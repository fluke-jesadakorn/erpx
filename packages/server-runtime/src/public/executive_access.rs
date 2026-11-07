//! Owning operations must validate current source/actor and revalidate after
//! staging this effect. This port grants no business read or command authority.
pub const CONTRACT_VERSION: &str = "runtime.executive-evidence-access.v1";
pub use crate::adapters::executive_access::record_executive_evidence_access;
pub use crate::domain::executive_access::{
    EvidenceAccessAction, EvidenceAccessRow, ExecutiveEvidenceAccess,
};
