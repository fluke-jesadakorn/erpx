//! Exact original notifications on the caller's current owner transaction.
//! The writer does not grant Work access or create an independent receipt.
pub use crate::adapters::maintenance_notifications::{
    OwnedPreparedMaintenanceNotification, PgMaintenanceNotificationSource,
    PreparedMaintenanceNotification, RetainedMaintenanceNotification,
    lock_maintenance_notification_sources, prepare_maintenance_notification,
};
pub use crate::domain::maintenance_notifications::{
    MaintenanceNotification, MaintenanceNotificationFacts, MaintenanceNotificationKind,
    derive_maintenance_notification,
};
