//! Exact original updated Field Service notification on the owner transaction.
pub use crate::adapters::field_service_notifications::{
    OwnedPreparedFieldServiceNotification, PgFieldServiceNotificationSource,
    PreparedFieldServiceNotification, RetainedFieldServiceNotification,
    lock_field_service_notification_sources, prepare_field_service_created_notification,
    prepare_field_service_notification,
};
pub use crate::domain::field_service_notifications::{
    FieldServiceNotification, FieldServiceNotificationFacts,
    derive_field_service_created_notification, derive_field_service_notification,
};
