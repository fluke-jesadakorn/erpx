//! Native position-move command contract.
//!
//! Consumers use these named exports; no SQL relation or private adapter type
//! escapes the crate. Scheduling and cancellation are owner commands; applying
//! a scheduled move remains a separate worker task.
pub use crate::adapters::position_moves_http::{
    position_moves_router, CancelPositionMoveHttpRequest, PositionMoveHttpBackend,
    PositionMoveHttpFuture, PositionMoveHttpState, PositionMoveHttpTarget,
    PositionMovePreviewHttpRequest, PositionMoveSubmitHttpRequest,
};
pub use crate::adapters::postgres_position_moves::{
    drain_due_position_moves, PgPositionMoveDrain, PgPositionMoves,
};
pub use crate::application::position_moves::{
    cancel_position_move, drain_once, position_moves, preview_digest, preview_position_move,
    require_permission, submit_position_move, ClaimedPositionMove, DrainFailure, DrainOutcome,
    FixedClock, PositionMoveDrainRepository, PositionMoveFuture, PositionMoveRepository,
};
pub use crate::domain::position_moves::permission_required as position_move_permission_required;
pub use crate::domain::position_moves::position_move_not_found;
pub use crate::domain::position_moves::{
    hash_value, leave_default_recommendation, position_gpo_matches, position_move_graph_issues,
    position_move_leave_impact, prepare_move, scoped, ExistingLeaveFact, GpoRule, Leave,
    LeaveAssignmentPlan, LeaveDefault, LeaveImpact, LeavePolicyFact, ManagerAssignment, MoveLevel,
    MoveMember, MoveNode, MovePerson, MovePosition, PositionMoveFacts, PositionMoveInput,
    PositionMovePersonWire, PositionMovePolicyChangesWire, PositionMovePolicyImpactWire,
    PositionMovePolicySourceWire, PositionMovePreview, PositionMoveRecord, PositionMoveTargetKind,
    PositionPlacementWire, PreparedPositionMove, PREVIEW_TTL_MS,
};
