//! Native roster Position-exit use case.
//!
//! This mirrors the retained `rosterPositionExitStatements` application: no
//! exiting engagement means no effects at all, otherwise one validated plan is
//! staged in the caller-owned serializable transaction. The retained ordering
//! interleaves Collaboration notices with People writes, so this layer exposes
//! the exact People phases and lets composition place the notices:
//!
//! ```text
//! locks -> weeks -> release history -> week notice
//!   -> assignments released -> request history -> request notice
//!   -> requests invalidated
//! ```
use crate::domain::roster_position_exit::RosterPositionExitPlan;
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};

/// One retained effect of a Position change, in execution order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RosterPositionExitPhase {
    /// `UPDATE people_roster_locks SET version=version+1`.
    Locks,
    /// `UPDATE people_roster_weeks SET version=version+1,updated_at=?`.
    Weeks,
    /// `INSERT people_roster_history 'assignment.released.position'`.
    ReleaseHistory,
    /// Collaboration `roster_week` staff notices.
    WeekNotice,
    /// `UPDATE people_roster_assignments SET state='released'`.
    ReleaseAssignments,
    /// `INSERT people_roster_history 'request.invalidated.position'`.
    RequestHistory,
    /// Collaboration `roster_request` staff notices.
    RequestNotice,
    /// `UPDATE people_roster_requests SET state='invalidated'`.
    Requests,
}

impl RosterPositionExitPhase {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Locks => "locks",
            Self::Weeks => "weeks",
            Self::ReleaseHistory => "assignment.released.position",
            Self::WeekNotice => "notice.roster_week",
            Self::ReleaseAssignments => "assignments.released",
            Self::RequestHistory => "request.invalidated.position",
            Self::RequestNotice => "notice.roster_request",
            Self::Requests => "requests.invalidated",
        }
    }

    /// Whether this People owner writes the phase. The two Collaboration
    /// notices are composition calls, never People SQL.
    pub const fn people_owned(self) -> bool {
        !matches!(self, Self::WeekNotice | Self::RequestNotice)
    }
}

pub const ROSTER_POSITION_EXIT_ORDER: [RosterPositionExitPhase; 8] = [
    RosterPositionExitPhase::Locks,
    RosterPositionExitPhase::Weeks,
    RosterPositionExitPhase::ReleaseHistory,
    RosterPositionExitPhase::WeekNotice,
    RosterPositionExitPhase::ReleaseAssignments,
    RosterPositionExitPhase::RequestHistory,
    RosterPositionExitPhase::RequestNotice,
    RosterPositionExitPhase::Requests,
];

/// The retained phase order, including the two Collaboration interleaves.
pub const fn roster_position_exit_order() -> &'static [RosterPositionExitPhase] {
    &ROSTER_POSITION_EXIT_ORDER
}

/// Plan the retained Position-exit effects.
///
/// The retained builder returned no statements when the exiting engagement list
/// was empty; `Ok(None)` preserves that no-op. Every other input is validated by
/// the domain plan before any SQL runs.
pub fn roster_position_exit_plan(
    company_id: &CompanyId,
    actor_user_id: &str,
    engagement_ids: &[String],
    command_id: &str,
    reason: &str,
    now: TimestampMillis,
) -> Result<Option<RosterPositionExitPlan>, BusinessError> {
    if engagement_ids.is_empty() {
        return Ok(None);
    }
    RosterPositionExitPlan::new(
        company_id.clone(),
        actor_user_id,
        engagement_ids,
        command_id,
        reason,
        now,
    )
    .map(Some)
}
