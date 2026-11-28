//! Native Sales customer follow-up command interface.
pub use crate::adapters::followups::PgFollowups;
pub use crate::adapters::followups_http::{
    FollowupHttpBackend, FollowupHttpCommand, FollowupHttpFuture, FollowupHttpRequest,
    FollowupHttpResponse, FollowupHttpTarget, customer_followups_router,
};
pub use crate::application::followups::{
    ChangeFollowupPlan, CreateFollowupPlan, FollowupCommandRepository, FollowupFuture,
    FollowupPurpose, change_customer_followup, create_customer_followup,
};
pub use crate::domain::followups::{
    ACTION_CREATE, ChangeFollowup, CreateFollowup, FollowupAction, FollowupActor, FollowupOrder,
    FollowupReceipt, FollowupReceiptSnapshot, FollowupSource, MAX_TEXT, MIN_TEXT, access_changed,
    idempotency_conflict, not_found, owner_changed, owner_required, readback_invalid,
    replay_result, request_invalid, source_changed, source_not_found, source_required,
    valid_due_date, version_conflict,
};
