//! Portable native Learning eligibility handoff facts.
pub use crate::application::eligibility_facts::{
    EligibilityFuture, EligibilityReadRepository, learning_eligibility_consumer_facts,
    learning_eligibility_facts,
};
pub use crate::domain::eligibility_facts::{
    EligibilityCertificateRow, EligibilityConsumptionRow, EligibilityQuery, EligibilityScope,
    MAX_ELIGIBILITY_CERTIFICATES, MAX_ELIGIBILITY_CONSUMPTIONS, eligibility_facts_json,
};
