//! Portable Customer Authority duplicate-review contract.
pub use crate::adapters::duplicates::PgCustomerDuplicateReview;
pub use crate::application::duplicates::{
    DuplicateAccessFuture, DuplicateReviewCommandRepository, DuplicateReviewReadRepository,
    change_duplicate_review, duplicate_review_detail, duplicate_review_work,
    duplicate_review_workspace, request_duplicate_review,
};
pub use crate::domain::duplicates::{
    ChangePlan, DuplicateAction, DuplicateChange, DuplicateReceipt, DuplicateReviewCustomer,
    DuplicateReviewEventRow, DuplicateReviewRequest, DuplicateReviewRow, MAX_EVIDENCE,
    MAX_HISTORY_ROWS, MAX_WORKSPACE_ROWS, MergeBlockers, MergePartyFacts, MergePlan,
    REQUEST_OPERATION, RequestPlan, ValidatedDuplicateChange, ValidatedDuplicateReviewRequest,
    change_identity, change_invalid, change_plan, idempotency_conflict,
    independent_approver_required, merge_blocked, merge_plan, merge_requires_duplicate,
    merge_source_changed, readback_invalid, request_id, request_invalid, request_plan,
    review_changed, review_not_found, reviewer_unavailable, source_not_found,
};
pub use crate::domain::access::AccessActor;
