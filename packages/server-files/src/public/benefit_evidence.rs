//! Files-owned Benefits dependent/election evidence port facade.
pub use crate::adapters::benefit_evidence::{
    BENEFITS_EVIDENCE_COMPONENT_KIND, BENEFITS_EVIDENCE_POLICY_VERSION, PgBenefitsEvidence,
    benefits_business_request,
};
pub use crate::adapters::business_evidence::PreparedBusinessEvidence as PreparedBenefitEvidence;
