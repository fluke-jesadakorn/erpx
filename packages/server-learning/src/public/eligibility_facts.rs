//! Portable native Learning eligibility handoff facts and benefit consumption.
pub use crate::adapters::eligibility_facts::lock_learning_benefit_sources;
pub use crate::application::eligibility_facts::{
    BenefitCommandEnvelope, BenefitConsumptionRepository, EligibilityFuture,
    EligibilityReadRepository, consume_benefit, finalize_benefit_consumption,
    learning_eligibility_consumer_facts, learning_eligibility_facts,
    prepare_benefit_consumption, validate_benefit_consumption,
};
pub use crate::domain::eligibility_facts::{
    BENEFIT_CONSUMPTION_PURPOSE, BENEFIT_CONSUME_OPERATION, BenefitConsumptionActor,
    BenefitConsumptionCommand, BenefitConsumptionPlan, BenefitConsumptionReceipt,
    BenefitConsumptionRequest, EligibilityCertificateRow, EligibilityConsumptionRow,
    EligibilityQuery, EligibilityScope, MAX_ELIGIBILITY_CERTIFICATES, MAX_ELIGIBILITY_CONSUMPTIONS,
    PreparedBenefitConsumption, benefit_consume_identity, benefit_consumption_plan,
    benefit_certificate_expired, benefit_eligibility_required, certificate_source_json,
    eligibility_facts_json, learning_benefit_continuation, require_certificate_current,
    select_benefit_certificate,
};
