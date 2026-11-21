//! Collaboration Portal notification effect; caller retains real authority.
pub use crate::adapters::customer_portal_notification::{
    PortalNotificationAction, PortalNotificationFacts, PortalNotificationSource,
    lock_customer_portal_notifications, notify_customer_portal_staff,
};
