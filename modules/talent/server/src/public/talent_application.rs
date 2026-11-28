//! Portable Talent application and proposal contract.
pub use crate::adapters::talent_application::PgTalentApplications;
pub use crate::application::talent_application::{
    ApplicationFuture, TalentApplicationCommandRepository, change_application, change_proposal,
    create_application, create_proposal,
};
pub use crate::domain::requisition::TalentActor;
pub use crate::domain::talent_application::{
    ApplicationAction, ApplicationChange, ApplicationChangePlan, ApplicationEventRow,
    ApplicationReceipt, ApplicationRequest, ApplicationRow, CREATE_APPLICATION_OPERATION,
    CREATE_PROPOSAL_OPERATION, CandidateFact, CreateApplicationPlan, CreateProposalPlan,
    MAX_EVENT_ROWS, MAX_REASON, MAX_SCORE_BPS, MAX_WORKSPACE_ROWS, ProposalAction, ProposalChange,
    ProposalChangePlan, ProposalReceipt, ProposalRequest, ProposalRow, Recommendation,
    RequisitionFact, ValidatedApplication, ValidatedApplicationChange, ValidatedProposal,
    ValidatedProposalChange, access_changed, application_change_identity, application_change_plan,
    application_changed, application_id, application_idempotency_conflict, application_not_found,
    application_readback_invalid, candidate_processing_blocked, create_application_identity,
    create_application_plan, create_proposal_identity, create_proposal_plan,
    independent_review_required, owner_ineligible, owner_required, proposal_change_identity,
    proposal_change_plan, proposal_changed, proposal_id, proposal_idempotency_conflict,
    proposal_not_found, proposal_readback_invalid, proposal_required, request_invalid,
    require_current_candidate, require_open_requisition, require_owner_eligible, source_changed,
    source_invalid,
};
