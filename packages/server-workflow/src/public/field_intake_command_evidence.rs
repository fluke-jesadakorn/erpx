//! Planned source-ordered Field intake command evidence; never grants authority.
pub use crate::adapters::field_intake_command_evidence::{
    FieldIntakeCommandClock, FieldIntakeCommandGeneratedIds, FieldIntakeCommandIdSource,
    FieldIntakeOriginFacts, FieldIntakeReceiptAnchor, PlannedFieldIntakeCommandEvidence,
    RetainedFieldIntakeCommandEvidence, plan_field_intake_command_evidence,
    verify_historical_field_intake_command,
};
