//! CRM activity notice effect; source and authority stay with composition.
pub use crate::adapters::crm_activity_notification::{
    CrmActivityNotificationFacts, PgCrmActivityNotificationSource, PreparedCrmActivityNotification,
    RetainedCrmActivityNotification, lock_crm_activity_notifications,
    prepare_crm_activity_notification,
};
