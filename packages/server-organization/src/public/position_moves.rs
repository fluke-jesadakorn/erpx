//! Native position-move command contract.
//!
//! Consumers use these named exports; no SQL relation or private adapter type
//! escapes the crate. Scheduling and cancellation are owner commands; applying
//! a scheduled move remains a separate worker task.
pub use crate::adapters::position_moves_http::{
    CancelPositionMoveHttpRequest, PositionMoveHttpBackend, PositionMoveHttpFuture,
    PositionMoveHttpState, PositionMoveHttpTarget, PositionMoveListHttpRequest,
    PositionMovePreviewHttpRequest, PositionMoveSubmitHttpRequest, position_moves_router,
};
pub use crate::adapters::postgres_position_moves::{
    PgPositionMoveDrain, PgPositionMoves, drain_due_position_moves,
};
pub use crate::application::position_moves::{
    ClaimedPositionMove, DrainFailure, DrainOutcome, FixedClock, PositionMoveDrainRepository,
    PositionMoveFuture, PositionMoveRepository, cancel_position_move, drain_once, position_moves,
    preview_digest, preview_position_move, require_permission, submit_position_move,
};
pub use crate::domain::position_moves::permission_required as position_move_permission_required;
pub use crate::domain::position_moves::position_move_not_found;
pub use crate::domain::position_moves::{
    ExistingLeaveFact, GpoRule, Leave, LeaveAssignmentPlan, LeaveDefault, LeaveImpact,
    LeavePolicyFact, ManagerAssignment, MoveLevel, MoveMember, MoveNode, MovePerson, MovePosition,
    PREVIEW_TTL_MS, PositionMoveFacts, PositionMoveInput, PositionMovePersonWire,
    PositionMovePolicyChangesWire, PositionMovePolicyImpactWire, PositionMovePolicySourceWire,
    PositionMovePreview, PositionMoveRecord, PositionMoveTargetKind, PositionPlacementWire,
    PreparedPositionMove, hash_value, leave_default_recommendation, position_gpo_matches,
    position_move_graph_issues, position_move_leave_impact, prepare_move, scoped,
};
