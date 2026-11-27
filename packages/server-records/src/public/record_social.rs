//! Native Records mention and evidence-based suggestion policy.
pub use crate::domain::record_social::{
    AddPrincipalMentionInput, FIELD_WRITE_STATEMENT, MENTION_STATEMENT, MENTION_TARGET_ID_MAX,
    MENTION_TARGET_LABEL_MAX, MentionTargetKind, MentionWrite, OP_MENTION, OP_SUGGESTION,
    OP_SUGGESTION_DECIDE, SUGGESTION_STATE_STATEMENT, SUGGESTION_STATEMENT, SuggestionDecision,
    SuggestionDecisionInput, SuggestionInput, SuggestionRequest, SuggestionSource,
    decision_input_invalid, guard_social_owner, mention_input_invalid, mention_target_not_eligible,
    mention_target_unavailable, parse_proposed_value, parse_sources, plan_mention,
    plan_suggestion_request, suggestion_conflict, suggestion_input_invalid, suggestion_unavailable,
    validate_decision_input, validate_mention_input, validate_suggestion_input,
};
