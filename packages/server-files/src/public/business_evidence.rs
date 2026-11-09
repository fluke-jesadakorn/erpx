//! Files-owned adapter for the shared Records business-evidence port.
pub use crate::adapters::business_evidence::{
    PgBusinessEvidenceFiles, PreparedBusinessEvidence, evidence_context,
    lock_business_evidence_sources, policy_fact,
};
