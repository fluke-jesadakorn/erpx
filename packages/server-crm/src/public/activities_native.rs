//! Native CRM activity create / complete / cancel ports and the PostgreSQL
//! owner adapter with the owner HTTP ingress in `public::activities_http`.
//! `reassign` is staged by the owner adapter and exposed through a separate
//! backend method: composition must supply selected authority, Workflow replay,
//! Sales recipient eligibility, purchase and Collaboration notification in
//! the same transaction. Purchase enforcement and the Workflow ledger remain
//! in TypeScript until that mount lands.
pub use crate::adapters::activities_native::{
    CrmReassignNotificationFacts, PgCrmActivityCommands, PreparedCrmActivityReassign,
    RetainedCrmActivityReassign,
};
pub use crate::application::activities_native::{
    ActivitiesFuture, ActivityFacts, ActivityRepository, LeadFacts, change_activity,
    create_activity,
};
pub use crate::domain::activities_native::{
    ActivityActor, ActivityChangeInput, ActivityChangePlan, ActivityCreateInput,
    ActivityCreatePlan, ActivityReassignIntent, ActivityReassignUtf16Input,
    ActivityReassignWireInput, CREATE_OPERATION, create_record_id,
    normalize_activity_reassign_utf16, normalized_activity_reassign_input,
    prepare_activity_reassign, require_lead_open, validate_change, validate_create,
};
