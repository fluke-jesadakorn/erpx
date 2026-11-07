//! Portable Customer Authority duplicate-review contract.
pub use crate::adapters::duplicates::PgCustomerDuplicateReview;
pub use crate::application::duplicates::{
    DuplicateAccessFuture, DuplicateReviewCommandRepository, DuplicateReviewReadRepository,
    duplicate_review_detail, duplicate_review_workspace, request_duplicate_review,
};
pub use crate::domain::duplicates::{
    DuplicateReviewCustomer, DuplicateReviewEventRow, DuplicateReviewRequest, DuplicateReviewRow,
    MAX_EVIDENCE, MAX_HISTORY_ROWS, MAX_WORKSPACE_ROWS, REQUEST_OPERATION, RequestPlan,
    ValidatedDuplicateReviewRequest, idempotency_conflict, readback_invalid, request_id,
    request_invalid, request_plan, review_changed, review_not_found, source_not_found,
};
pub use crate::domain::access::AccessActor;
