//! CRM activity notice effect; source and authority stay with composition.
pub use crate::adapters::crm_activity_notification::{
    CrmActivityNotificationFacts, CrmLeadNotificationFacts, PgCrmActivityNotificationSource,
    PgCrmLeadNotificationSource, PreparedCrmActivityNotification, PreparedCrmLeadNotification,
    RetainedCrmActivityNotification, RetainedCrmLeadNotification, lock_crm_activity_notifications,
    prepare_crm_activity_notification, prepare_crm_lead_notification,
};
