//! Bounded native Records mention owner policy.
//!
//! Composition binds the actor and clock, resolves the cross-owner principal,
//! and owns the record-version guard, transaction, receipt, audit and outbox.
pub use crate::domain::record_social::{
    AddPrincipalMentionInput, MENTION_STATEMENT, MENTION_TARGET_ID_MAX, MENTION_TARGET_LABEL_MAX,
    MentionTargetKind, MentionWrite, OP_MENTION, mention_input_invalid,
    mention_target_not_eligible, mention_target_unavailable, plan_mention, validate_mention_input,
};
