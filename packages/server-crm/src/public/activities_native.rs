//! Native CRM activity create / complete / cancel ports and the PostgreSQL
//! owner adapter. `reassign` remains TS-owned (cross-owner
//! `sales_recipient_eligibility` join); purchase enforcement and the Workflow
//! ledger remain in TypeScript.
pub use crate::adapters::activities_native::PgCrmActivityCommands;
pub use crate::application::activities_native::{
    ActivitiesFuture, ActivityFacts, ActivityRepository, LeadFacts, change_activity, create_activity,
};
pub use crate::domain::activities_native::{
    ActivityActor, ActivityChangeInput, ActivityChangePlan, ActivityCreateInput,
    ActivityCreatePlan, CREATE_OPERATION, create_record_id, require_lead_open, validate_change,
    validate_create,
};
