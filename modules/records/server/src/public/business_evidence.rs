pub const CONTRACT_VERSION: &str = "records.business-evidence.v1";
pub use crate::application::business_evidence::{
    BusinessEvidenceBinding, BusinessEvidenceFilesPort, BusinessEvidenceRequest,
    prepare_business_evidence,
};
pub use crate::domain::business_evidence::{
    EvidenceContext, EvidencePolicyFact, EvidenceSelection, EvidenceSetFact, select_evidence,
    unique_evidence_ids,
};
