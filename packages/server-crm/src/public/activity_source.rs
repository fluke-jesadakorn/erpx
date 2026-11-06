//! Bounded CRM-owned source identity, not a full purchase or actor authority.
pub use crate::adapters::activity_source::{
    CrmActivitySourceKind, CrmActivitySourceRequest, CurrentCrmActivitySource,
    inspect_crm_activity_source, lock_crm_activity_sources,
};
