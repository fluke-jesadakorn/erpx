//! Native CRM lead / opportunity command ports and PostgreSQL adapter.
pub use crate::adapters::leads_native::PgCrmLeadCommands;
pub use crate::application::leads_native::{
    LeadFacts, LeadRepository, LeadsFuture, OpportunityFacts, change_lead, change_opportunity,
    create_lead,
};
pub use crate::domain::leads_native::{
    LeadActor, LeadChangeInput, LeadChangePlan, LeadCreateInput, LeadCreatePlan,
    OpportunityChangeInput, OpportunityChangePlan, require_lead_state_for_change,
    require_opportunity_state, validate_lead_change, validate_lead_create,
    validate_opportunity_change,
};
