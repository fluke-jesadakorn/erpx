//! Native Sales customer-notification settings/queue/retry commands.
pub use crate::adapters::notification_commands::PgNotificationCommands;
pub use crate::adapters::notification_commands_http::{
    NotificationHttpBackend, NotificationHttpCommand, NotificationHttpFuture,
    NotificationHttpRequest, NotificationHttpResponse, NotificationHttpTarget,
    sales_notification_commands_router,
};
pub use crate::application::notification_commands::{
    NotificationCommandRepository, NotificationFuture, NotificationPurpose, QueuePlan,
    RetryPlan, SettingsPlan, queue_notification, retry_notification, update_settings,
};
pub use crate::domain::notifications::{
    DeliveryFacts, NotificationActor, NotificationReceipt, NotificationReceiptSnapshot,
    NoticeSource, QueueNotification, RetryNotification, SettingsAdapter, SettingsReceipt,
    UpdateSettings, access_changed, already_notified, consent_required, delivery_not_found,
    feature_unavailable, idempotency_conflict, message, not_configured, not_failed,
    notification_not_found, provider_required, readback_invalid, replay_notice, replay_settings,
    request_invalid, source_changed, version_conflict,
};
