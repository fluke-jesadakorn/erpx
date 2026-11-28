//! Collaboration-owned roster Position-change notices (the retained
//! `roster_week` and `roster_request` staff inbox rows).
//!
//! Composition calls these inside the same serializable transaction as the
//! People Position-exit effects, between the retained People phases.
pub use crate::adapters::roster_position_notifications::{
    ROSTER_POSITION_NOTICE_MAX_ENGAGEMENTS, ROSTER_POSITION_REQUEST_MESSAGE,
    ROSTER_POSITION_WEEK_MESSAGE, RosterPositionExitNotices, roster_position_notice_invalid,
    roster_position_request_notice_sql, roster_position_week_notice_sql,
    validate_roster_position_notice_input, write_roster_position_exit_notices,
    write_roster_position_exit_request_notices, write_roster_position_exit_week_notices,
};
