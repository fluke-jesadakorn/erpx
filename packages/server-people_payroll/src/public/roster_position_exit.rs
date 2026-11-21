//! Native People roster Position-exit effects: retained lock/week version bumps,
//! released-position and invalidated-request history, assignment release and
//! pending request invalidation.
//!
//! The caller owns authorization and the serializable Holding transaction.
//! Composition stages the People phases in [`roster_position_exit_order`] and
//! writes the two Collaboration notices between them; this owner never writes
//! `staff_notifications`.
pub use crate::adapters::roster_position_exit_postgres::{
    RosterPositionExitAffected, RosterPositionExitInvalidated, RosterPositionExitLocked,
    RosterPositionExitReleased, roster_position_exit_affected,
    stage_roster_position_exit_invalidate_requests, stage_roster_position_exit_lock_and_history,
    stage_roster_position_exit_release_and_history,
};
pub use crate::application::roster_position_exit::{
    ROSTER_POSITION_EXIT_ORDER, RosterPositionExitPhase, roster_position_exit_order,
    roster_position_exit_plan,
};
pub use crate::domain::roster_position_exit::{
    ROSTER_POSITION_EXIT_INVALIDATED_ACTION, ROSTER_POSITION_EXIT_MAX_COMMAND,
    ROSTER_POSITION_EXIT_MAX_ENGAGEMENTS, ROSTER_POSITION_EXIT_MAX_REASON,
    ROSTER_POSITION_EXIT_MIN_COMMAND, ROSTER_POSITION_EXIT_MIN_REASON,
    ROSTER_POSITION_EXIT_RELEASED_ACTION, RosterPositionExitPlan, affected_assignments,
    pending_position_requests, roster_position_exit_invalid,
};
