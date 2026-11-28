//! Versioned owner effect; composition must retain actual Field lineage, Files and purchase proofs.
pub const CONTRACT_VERSION: &str = "finance.field-journal-evidence.v1";
pub use crate::adapters::field_journal_evidence::{
    PgFieldJournalEvidenceAuthority, PreparedFieldJournalEvidenceVersion,
    prepare_field_journal_evidence_version,
};
