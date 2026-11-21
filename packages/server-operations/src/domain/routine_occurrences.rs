//! Owner-local scheduled routine occurrence facts.
//!
//! The pure planner reproduces the retained `generateOccurrences` unit: pause
//! skips, prior-occurrence deduplication, next cursor, due offset, blocked
//! state and the generated notification text. Cross-owner authority, purchase
//! and route facts arrive through the application ports, never from the request.
use crate::domain::routine_commands::{
    RoutineFrequency, RoutineRule, RoutineScheduleRow, RoutineTemplateRow, next_occurrence,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct OccurrenceSourceFacts {
    pub pause_exists: bool,
    pub pause_ended_at: Option<i64>,
    pub prior: bool,
    pub start_blocked: Option<String>,
    pub completion_blocked: Option<String>,
    pub project_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OccurrencePlan {
    pub key: String,
    pub task_id: String,
    pub next: Option<i64>,
    pub due_at: i64,
    pub state: String,
    pub blocked_reason: Option<String>,
    pub notification: String,
    pub creator_notification: Option<String>,
    pub open_start_approval: bool,
    pub start_approvers: Vec<String>,
    pub completion_approvers: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OccurrenceOutcome {
    pub next: Option<i64>,
    pub generated: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OccurrencePass {
    pub generated: i64,
    pub examined: i64,
}

/// Caller-owned selection predicate. `None` means the retained development
/// `TRUE` scope; `Some` is the code-owned queued authorization/purchase scope
/// for one Holding, including any production rollout predicate.
#[derive(Clone, Debug, Default)]
pub struct OccurrencePolicy {
    pub production: bool,
    pub scope_sql: Option<String>,
}

fn rule(schedule: &RoutineScheduleRow) -> Result<RoutineRule, BusinessError> {
    let frequency = match schedule.frequency.as_str() {
        "daily" => RoutineFrequency::Daily,
        "weekly" => RoutineFrequency::Weekly,
        "monthly" => RoutineFrequency::Monthly,
        _ => {
            return Err(BusinessError::invalid(
                "ROUTINE_FIELDS_INVALID",
                "Review the routine fields.",
            ));
        }
    };
    Ok(RoutineRule {
        frequency,
        timezone: schedule.timezone.clone(),
        occurrence_time: schedule.occurrence_time.clone(),
        start_date: schedule.start_date.clone(),
        end_date: schedule.end_date.clone(),
        weekday: schedule.weekday,
        month_day: schedule.month_day,
    })
}

/// The retained `occurrenceStatements` decision for one due occurrence.
///
/// A pause row or an existing task at the same instant advances the cursor
/// without generating. A blocked start/completion route or an unavailable
/// Project produces a draft, and the start approval opens only for a ready
/// `start_approval` occurrence.
pub fn plan_occurrence(
    template: &RoutineTemplateRow,
    schedule: &RoutineScheduleRow,
    facts: &OccurrenceSourceFacts,
    at: i64,
) -> Result<OccurrencePlan, BusinessError> {
    let next = next_occurrence(&rule(schedule)?, facts.pause_ended_at.unwrap_or(at))?;
    let key = format!("occurrence-{}-{at}", schedule.id);
    let task_id = format!("task-{}-{at}", schedule.id);
    if facts.pause_exists || facts.prior {
        return Ok(OccurrencePlan {
            key,
            task_id,
            next,
            due_at: at + schedule.due_offset_hours * 3_600_000,
            state: String::new(),
            blocked_reason: None,
            notification: String::new(),
            creator_notification: None,
            open_start_approval: false,
            start_approvers: Vec::new(),
            completion_approvers: Vec::new(),
        });
    }
    let start_approvers = template.start_approvers()?;
    let completion_approvers = template.completion_approvers()?;
    let mut blocked = facts
        .start_blocked
        .clone()
        .or_else(|| facts.completion_blocked.clone());
    if template.project_id.is_some() && !facts.project_available {
        blocked.get_or_insert_with(|| "The project is unavailable.".into());
    }
    let state = if blocked.is_some() {
        "draft"
    } else if !start_approvers.is_empty() {
        "start_approval"
    } else {
        "ready"
    };
    let notification = if blocked.is_some() {
        "Routine task blocked: correction required"
    } else {
        "Routine task assigned to you"
    };
    let creator_notification = blocked
        .as_ref()
        .map(|reason| format!("Routine blocked: {reason}"));
    Ok(OccurrencePlan {
        key,
        task_id,
        next,
        due_at: at + schedule.due_offset_hours * 3_600_000,
        state: state.into(),
        blocked_reason: blocked,
        notification: notification.into(),
        creator_notification,
        open_start_approval: state == "start_approval",
        start_approvers,
        completion_approvers,
    })
}

/// Stored `work_routine_execution_authorizations` row. The authority JSON is
/// the retained minimal StaffActor image; optional `iam`/`authority` objects
/// are honored when a producer captured them.
#[derive(Clone, Debug)]
pub struct RoutineExecutionAuthorization {
    pub command_id: String,
    pub schedule_id: String,
    pub template_id: String,
    pub company_id: String,
    pub actor_id: String,
    pub assignment_id: String,
    pub authority_json: String,
    pub source_hash: String,
    pub template_version: i64,
    pub schedule_version: i64,
    pub initial_occurrence_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct RoutineExecutionStep {
    pub command_id: String,
    pub occurrence_at: i64,
    pub next_occurrence_at: Option<i64>,
    pub task_id: Option<String>,
    pub generated: bool,
    pub source_hash: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRoutineAuthority {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub assignment_id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub company_id: String,
    #[serde(default)]
    pub branch_id: Option<String>,
    #[serde(default)]
    pub department_id: Option<String>,
    #[serde(default)]
    pub assignment_version: Option<i64>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub authority: Option<StoredRoutineSession>,
    #[serde(default)]
    pub iam: Option<StoredRoutineIam>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRoutineSession {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub revision: Option<i64>,
    #[serde(default)]
    pub assignment_version: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRoutineIam {
    #[serde(default)]
    pub queued_assignment_version: Option<i64>,
    #[serde(default)]
    pub role_revision: Option<i64>,
    #[serde(default)]
    pub identity_version: Option<i64>,
    #[serde(default)]
    pub emergency_until: Option<i64>,
    #[serde(default)]
    pub hr_appointment_version: Option<i64>,
}

impl RoutineExecutionAuthorization {
    pub fn actor(&self) -> Result<StoredRoutineAuthority, BusinessError> {
        serde_json::from_str(&self.authority_json).map_err(|_| {
            BusinessError::new(
                FailureKind::Conflict,
                "WORK_ROUTINE_SOURCE_CHANGED",
                "The original queued routine configuration changed. Pause and resume it explicitly.",
            )
        })
    }

    /// `authority.assignmentVersion` wins, then the captured IAM queued
    /// assignment version, matching `currentAssignmentReadScope`.
    pub fn assignment_version(&self, actor: &StoredRoutineAuthority) -> Option<i64> {
        actor
            .authority
            .as_ref()
            .and_then(|authority| authority.assignment_version)
            .or_else(|| {
                actor
                    .iam
                    .as_ref()
                    .and_then(|iam| iam.queued_assignment_version)
            })
            .or(actor.assignment_version)
    }
}

#[derive(Clone, Debug)]
pub struct OccurrenceSource {
    pub schedule: RoutineScheduleRow,
    pub template: Option<RoutineTemplateRow>,
    pub pause_exists: bool,
    pub pause_ended_at: Option<i64>,
    pub prior: bool,
}

/// RFC-3339 image used only to satisfy the shared task request validation;
/// the owner write stores the exact due epoch milliseconds supplied separately.
pub fn due_at_iso(millis: i64) -> Result<String, BusinessError> {
    jiff::Timestamp::from_millisecond(millis)
        .map(|value| value.to_string())
        .map_err(|_| {
            BusinessError::invalid(
                "ROUTINE_FIELDS_INVALID",
                "Review the routine fields.",
            )
        })
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_ROUTINE_SOURCE_CHANGED",
        "The original scheduled occurrence lineage changed.",
    )
}

pub fn authorization_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_ROUTINE_AUTHORIZATION_REQUIRED",
        "Pause and resume this retained schedule with its original current owner before generation.",
    )
}
