//! Original Maintenance work-state policy. These facts and projections are not
//! current authorization, an inventory capability or an owner write command.
use erpx_server_shared::public::{BusinessError, FailureKind, UserId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceWorkFacts {
    pub owner_id: UserId,
    pub status: String,
    pub owner_eligible: bool,
    pub reviewer_eligible: bool,
    pub scheduled_at: Option<i64>,
    pub downtime_started_at: Option<i64>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceActorFacts {
    pub user_id: UserId,
    pub can_review: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaintenanceTransition {
    Schedule { owner_id: UserId, scheduled_at: i64 },
    Start,
    Block,
    Resume,
    Complete,
    IssuePart,
    Cancel,
}
impl MaintenanceTransition {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Schedule { .. } => "schedule",
            Self::Start => "start",
            Self::Block => "block",
            Self::Resume => "resume",
            Self::Complete => "complete",
            Self::IssuePart => "issue-part",
            Self::Cancel => "cancel",
        }
    }
    pub fn allowed_states(&self) -> &'static [&'static str] {
        match self {
            Self::Schedule { .. } | Self::Start => &["open", "scheduled"],
            Self::Block | Self::Complete | Self::IssuePart => &["in_progress"],
            Self::Resume => &["blocked"],
            Self::Cancel => &["open", "scheduled", "in_progress", "blocked"],
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceState {
    pub status: &'static str,
    pub owner_id: UserId,
    pub scheduled_at: Option<i64>,
    pub downtime_started_at: Option<i64>,
    pub downtime_ended_at: Option<i64>,
}
/// Preserve owner/reviewer refusal before current-state refusal. Operational
/// eligibility must come from actual retained People/IAM owner ports; callers
/// cannot use these plain facts as proof to mutate an inventory source.
pub fn require_maintenance_transition(
    actor: &MaintenanceActorFacts,
    source: &MaintenanceWorkFacts,
    transition: &MaintenanceTransition,
) -> Result<(), BusinessError> {
    let own = source.owner_id == actor.user_id && source.owner_eligible;
    let review = actor.can_review && source.reviewer_eligible;
    if !own && !review {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MAINTENANCE_OWNER_REQUIRED",
            "The current owner or an eligible reviewer must perform this action.",
        ));
    }
    if !transition
        .allowed_states()
        .contains(&source.status.as_str())
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "MAINTENANCE_STATE_CHANGED",
            "Refresh the work order before continuing.",
        ));
    }
    Ok(())
}
/// Pure Work aggregate projection only. Asset changes, reason/evidence,
/// request/schema/version, purchases and atomic owner effects are separate.
pub fn maintenance_state(
    source: &MaintenanceWorkFacts,
    transition: &MaintenanceTransition,
    now: i64,
) -> MaintenanceState {
    let status = match transition {
        MaintenanceTransition::Schedule { .. } => "scheduled",
        MaintenanceTransition::Start
        | MaintenanceTransition::Resume
        | MaintenanceTransition::IssuePart => "in_progress",
        MaintenanceTransition::Block => "blocked",
        MaintenanceTransition::Complete => "completed",
        MaintenanceTransition::Cancel => "cancelled",
    };
    let (owner_id, scheduled_at) = match transition {
        MaintenanceTransition::Schedule {
            owner_id,
            scheduled_at,
        } => (owner_id.clone(), Some(*scheduled_at)),
        _ => (source.owner_id.clone(), source.scheduled_at),
    };
    let downtime_started_at = if matches!(transition, MaintenanceTransition::Start) {
        Some(now)
    } else {
        source.downtime_started_at
    };
    // B0's nullable epoch field uses JavaScript truthiness. Epoch zero is
    // retained as a start value but does not create completion/cancel downtime.
    let downtime_ended_at = if matches!(
        transition,
        MaintenanceTransition::Complete | MaintenanceTransition::Cancel
    ) && source.downtime_started_at.is_some_and(|v| v != 0)
    {
        Some(now)
    } else {
        None
    };
    MaintenanceState {
        status,
        owner_id,
        scheduled_at,
        downtime_started_at,
        downtime_ended_at,
    }
}
