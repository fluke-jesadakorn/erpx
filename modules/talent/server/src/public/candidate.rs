//! Portable Talent candidate and privacy contract.
pub use crate::adapters::candidate::PgTalentCandidates;
pub use crate::application::candidate::{
    CandidateFuture, TalentCandidateCommandRepository, TalentCandidateReadRepository,
    candidate_privacy_workspace, change_candidate_privacy, create_candidate,
};
pub use crate::domain::candidate::{
    CREATE_OPERATION, CandidateReceipt, CandidateRequest, CandidateRow, CreatePlan,
    ERASE_PERMISSION, HOLD_PERMISSION, MAX_EMAIL, MAX_LEGAL_NAME, MAX_PHONE, MAX_PRIVACY_EVENTS,
    MAX_PRIVACY_REASON, MIN_PRIVACY_REASON, PrivacyAction, PrivacyEventRow, PrivacyPlan,
    PrivacyRequest, RELEASE_PERMISSION, ValidatedCandidate, ValidatedPrivacy, WITHDRAW_PERMISSION,
    access_changed, candidate_changed, candidate_id, candidate_not_found,
    company_hr_scope_required, create_identity, create_plan, erasure_blocked, idempotency_conflict,
    privacy_forbidden, privacy_identity, privacy_plan, privacy_state, privacy_view,
    readback_invalid, request_invalid, require_privacy_permission, require_privacy_scope,
    require_privacy_view, retention_invalid, self_approval, source_invalid,
};
pub use crate::domain::requisition::TalentActor;
