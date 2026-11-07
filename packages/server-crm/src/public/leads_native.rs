//! Native CRM lead / opportunity command ports and PostgreSQL adapter.
pub use crate::adapters::leads_native::{
    ChangedCrmLeadFacts, ChangedCrmOpportunityFacts, PgCrmLeadCommands, PreparedLeadChange,
    PreparedOpportunityChange, QualifiedCrmLeadFacts, RetainedCrmLeadChange,
    RetainedCrmOpportunityChange, RetainedCrmQualification, apply_crm_qualification,
    apply_crm_qualification_retained, deal_event_id, lead_event_id,
};
pub use crate::application::leads_native::{
    LeadFacts, LeadRepository, LeadsFuture, OpportunityFacts, change_lead, change_opportunity,
    create_lead,
};
pub use crate::domain::leads_native::{
    LeadActor, LeadChangeInput, LeadChangePlan, LeadChangeReceipt, LeadCreateInput, LeadCreatePlan,
    NATIVE_LEAD_ACTIONS, NATIVE_OPPORTUNITY_ACTIONS, OpportunityChangeInput, OpportunityChangePlan,
    OpportunityChangeReceipt, lead_operation, native_lead_action, native_opportunity_action,
    not_native, opportunity_operation, require_lead_state_for_change, require_opportunity_state,
    validate_lead_change, validate_lead_create, validate_opportunity_change,
};
