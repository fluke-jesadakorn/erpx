//! Portable Support evidence binding and withdrawal contract.
pub use crate::application::support_evidence::{
    EvidencePorts, bind_evidence, revoke_evidence,
};
pub use crate::domain::support_evidence::{
    ALLOWED_PURPOSES, EvidenceActor, EvidenceBind, EvidenceBindPlan, EvidenceLinkFacts,
    EvidenceRevoke, EvidenceRevokePlan, EvidenceSetFacts, EvidenceTicket, MAX_REASON,
    evidence_active, plan_bind, plan_revoke,
};
