//! Finite native Field-parts journal responsibility contract.
pub const CONTRACT_VERSION: &str = "workflow.field-journal-flow.v1";
pub use crate::adapters::field_journal_flow::{
    FieldJournalFlowAssignment, FieldJournalFlowAssignmentInput, FieldJournalFlowBasis,
    lock_field_journal_flow_sources, read_field_journal_flow_basis,
};
pub use crate::domain::field_journal_flow::field_journal_flow_view;
