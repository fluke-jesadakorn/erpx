use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::{AuthorityScope, DataClassification};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_NOTIFICATION_CHANNELS: usize = 5;
pub const MAX_NOTIFICATION_CALLBACKS: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannel {
    InApp,
    Email,
    Sms,
    Push,
    Webhook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxState {
    Unread,
    Read,
    Acknowledged,
    ActionRequired,
    Withdrawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Queued,
    Accepted,
    Delivered,
    Failed,
    Uncertain,
    Suppressed,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NotificationError {
    #[error("notification input is invalid")]
    Invalid,
    #[error("notification authority is denied")]
    Denied,
    #[error("notification lifecycle conflict")]
    Conflict,
    #[error("notification provider is unavailable")]
    ProviderUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPreferences {
    pub enabled_channels: HashSet<NotificationChannel>,
    pub muted_source_ids: HashSet<String>,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub quiet_hours_active: bool,
    pub timezone_validated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MandatoryDeliveryPolicy {
    pub policy_version: String,
    pub required_channels: HashSet<NotificationChannel>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryAttempt {
    pub channel: NotificationChannel,
    pub delivery_version: u64,
    pub dedupe_key: String,
    pub state: DeliveryState,
    pub provider_message_ref: Option<String>,
    pub last_provider_sequence: u64,
    pub processed_callback_ids: Vec<String>,
    pub attempt_count: u16,
    pub last_error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub notification_id: Uuid,
    pub version: u64,
    pub delivery_version: u64,
    pub scope: AuthorityScope,
    pub source_id: String,
    pub source_record_ref: String,
    pub recipient_ref: String,
    pub classification: DataClassification,
    pub preview: Option<String>,
    pub inbox_state: InboxState,
    pub mandatory_policy_version: Option<String>,
    pub attempts: Vec<DeliveryAttempt>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Notification {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        notification_id: Uuid,
        scope: AuthorityScope,
        source_id: String,
        source_record_ref: String,
        recipient_ref: String,
        classification: DataClassification,
        requested_preview: Option<String>,
        preview_authorized: bool,
        action_required: bool,
        at: DateTime<Utc>,
    ) -> Result<Self, NotificationError> {
        if notification_id.is_nil()
            || scope.validate().is_err()
            || !valid_text(&source_id)
            || !valid_text(&source_record_ref)
            || !valid_text(&recipient_ref)
            || requested_preview
                .as_deref()
                .is_some_and(|value| !valid_preview(value))
        {
            return Err(NotificationError::Invalid);
        }
        Ok(Self {
            notification_id,
            version: 1,
            delivery_version: 1,
            scope,
            source_id,
            source_record_ref,
            recipient_ref,
            classification,
            preview: preview_authorized.then_some(requested_preview).flatten(),
            inbox_state: if action_required {
                InboxState::ActionRequired
            } else {
                InboxState::Unread
            },
            mandatory_policy_version: None,
            attempts: Vec::new(),
            created_at: at,
            updated_at: at,
        })
    }

    pub fn plan_deliveries(
        &mut self,
        preferences: &NotificationPreferences,
        mandatory: Option<&MandatoryDeliveryPolicy>,
        at: DateTime<Utc>,
    ) -> Result<Vec<DeliveryAttempt>, NotificationError> {
        if self.inbox_state == InboxState::Withdrawn {
            return Err(NotificationError::Conflict);
        }
        let mut channels = HashSet::from([NotificationChannel::InApp]);
        let preference_suppressed = preferences.muted_source_ids.contains(&self.source_id)
            || preferences.snoozed_until.is_some_and(|until| until > at)
            || preferences.quiet_hours_active
            || !preferences.timezone_validated;
        if !preference_suppressed {
            channels.extend(preferences.enabled_channels.iter().copied());
        }
        if let Some(policy) = mandatory {
            if !valid_text(&policy.policy_version)
                || !valid_text(&policy.reason)
                || policy.required_channels.len() > MAX_NOTIFICATION_CHANNELS
            {
                return Err(NotificationError::Invalid);
            }
            channels.extend(policy.required_channels.iter().copied());
            self.mandatory_policy_version = Some(policy.policy_version.clone());
        }
        let delivery_version = self.delivery_version;
        let mut added = Vec::new();
        for channel in channels {
            let key = format!(
                "{}:{}:{}:{}",
                self.notification_id,
                self.recipient_ref,
                channel_name(channel),
                delivery_version
            );
            if self
                .attempts
                .iter()
                .any(|attempt| attempt.dedupe_key == key)
            {
                continue;
            }
            let attempt = DeliveryAttempt {
                channel,
                delivery_version,
                dedupe_key: key,
                state: if channel == NotificationChannel::InApp {
                    DeliveryState::Delivered
                } else {
                    DeliveryState::Queued
                },
                provider_message_ref: None,
                last_provider_sequence: 0,
                processed_callback_ids: Vec::new(),
                attempt_count: 0,
                last_error_code: None,
            };
            self.attempts.push(attempt.clone());
            added.push(attempt);
        }
        if !added.is_empty() {
            self.version = self
                .version
                .checked_add(1)
                .ok_or(NotificationError::Invalid)?;
            self.updated_at = at;
        }
        Ok(added)
    }

    pub fn acknowledge(
        &mut self,
        principal_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<(), NotificationError> {
        if principal_ref != self.recipient_ref || self.inbox_state == InboxState::Withdrawn {
            return Err(NotificationError::Denied);
        }
        if self.inbox_state != InboxState::Acknowledged {
            self.inbox_state = InboxState::Acknowledged;
            self.version = self
                .version
                .checked_add(1)
                .ok_or(NotificationError::Invalid)?;
            self.updated_at = at;
        }
        Ok(())
    }

    pub fn record_provider_acceptance(
        &mut self,
        dedupe_key: &str,
        provider_message_ref: String,
        at: DateTime<Utc>,
    ) -> Result<(), NotificationError> {
        let attempt = self
            .attempts
            .iter_mut()
            .find(|attempt| attempt.dedupe_key == dedupe_key)
            .ok_or(NotificationError::Invalid)?;
        if attempt.state != DeliveryState::Queued || !valid_text(&provider_message_ref) {
            return Err(NotificationError::Conflict);
        }
        attempt.state = DeliveryState::Accepted;
        attempt.provider_message_ref = Some(provider_message_ref);
        attempt.attempt_count = attempt
            .attempt_count
            .checked_add(1)
            .ok_or(NotificationError::Invalid)?;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(NotificationError::Invalid)?;
        self.updated_at = at;
        Ok(())
    }

    pub fn reconcile_callback(
        &mut self,
        callback: DeliveryCallback,
        at: DateTime<Utc>,
    ) -> Result<bool, NotificationError> {
        if !callback.verified {
            return Err(NotificationError::Denied);
        }
        let attempt = self
            .attempts
            .iter_mut()
            .find(|attempt| {
                attempt.provider_message_ref.as_deref() == Some(&callback.provider_message_ref)
            })
            .ok_or(NotificationError::Denied)?;
        if attempt
            .processed_callback_ids
            .contains(&callback.callback_id)
        {
            return Ok(false);
        }
        if callback.sequence == 0
            || callback.sequence <= attempt.last_provider_sequence
            || callback.occurred_at > at
            || attempt.processed_callback_ids.len() >= MAX_NOTIFICATION_CALLBACKS
        {
            return Err(NotificationError::Conflict);
        }
        attempt.last_provider_sequence = callback.sequence;
        attempt.processed_callback_ids.push(callback.callback_id);
        attempt.state = callback.state;
        attempt.last_error_code = callback.error_code;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(NotificationError::Invalid)?;
        self.updated_at = at;
        Ok(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryCallback {
    pub callback_id: String,
    pub provider_message_ref: String,
    pub sequence: u64,
    pub state: DeliveryState,
    pub error_code: Option<String>,
    pub verified: bool,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDeliveryRequest {
    pub notification_id: Uuid,
    pub channel: NotificationChannel,
    pub dedupe_key: String,
    pub recipient_ref: String,
    pub classification: DataClassification,
    pub preview: Option<String>,
}

#[async_trait]
pub trait NotificationDeliveryPort: Send + Sync {
    async fn deliver(&self, request: ProviderDeliveryRequest) -> Result<String, NotificationError>;

    async fn reconcile_uncertain(
        &self,
        provider_message_ref: &str,
    ) -> Result<DeliveryState, NotificationError>;
}

#[async_trait]
pub trait NotificationRepositoryPort: Send + Sync {
    async fn load(&self, notification_id: Uuid) -> Result<Option<Notification>, NotificationError>;
    async fn save(
        &self,
        notification: &Notification,
        expected_version: u64,
    ) -> Result<(), NotificationError>;
}

pub fn group_recipient_snapshot(
    group_version: u64,
    recipients: Vec<String>,
    maximum: usize,
) -> Result<HashMap<String, u64>, NotificationError> {
    if group_version == 0 || recipients.len() > maximum {
        return Err(NotificationError::Denied);
    }
    let mut result = HashMap::new();
    for recipient in recipients {
        if !valid_text(&recipient) || result.insert(recipient, group_version).is_some() {
            return Err(NotificationError::Invalid);
        }
    }
    Ok(result)
}

fn channel_name(channel: NotificationChannel) -> &'static str {
    match channel {
        NotificationChannel::InApp => "in_app",
        NotificationChannel::Email => "email",
        NotificationChannel::Sms => "sms",
        NotificationChannel::Push => "push",
        NotificationChannel::Webhook => "webhook",
    }
}

fn valid_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 255 && !value.chars().any(char::is_control)
}
fn valid_preview(value: &str) -> bool {
    value.len() <= 1_024 && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn at() -> DateTime<Utc> {
        DateTime::from_timestamp(1_786_000_000, 0).unwrap()
    }
    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }
    fn notice(preview_authorized: bool) -> Notification {
        Notification::create(
            Uuid::from_u128(10),
            scope(),
            "finance".into(),
            "reference:item:invoice".into(),
            "recipient".into(),
            DataClassification::Confidential,
            Some("Invoice requires review".into()),
            preview_authorized,
            true,
            at(),
        )
        .unwrap()
    }

    #[test]
    fn inbox_is_authoritative_and_mandatory_policy_overrides_mute_without_leaking_preview() {
        let mut notification = notice(false);
        assert_eq!(notification.preview, None);
        let attempts = notification
            .plan_deliveries(
                &NotificationPreferences {
                    enabled_channels: HashSet::from([NotificationChannel::Email]),
                    muted_source_ids: HashSet::from(["finance".into()]),
                    snoozed_until: None,
                    quiet_hours_active: false,
                    timezone_validated: true,
                },
                Some(&MandatoryDeliveryPolicy {
                    policy_version: "legal-notice-v2".into(),
                    required_channels: HashSet::from([NotificationChannel::Email]),
                    reason: "mandatory legal notice".into(),
                }),
                at(),
            )
            .unwrap();
        assert!(
            attempts
                .iter()
                .any(|value| value.channel == NotificationChannel::InApp)
        );
        assert!(
            attempts
                .iter()
                .any(|value| value.channel == NotificationChannel::Email)
        );
        assert_eq!(notification.inbox_state, InboxState::ActionRequired);
    }

    #[test]
    fn delivery_deduplicates_and_verified_callbacks_advance_once() {
        let mut notification = notice(true);
        let preferences = NotificationPreferences {
            enabled_channels: HashSet::from([NotificationChannel::Email]),
            muted_source_ids: HashSet::new(),
            snoozed_until: None,
            quiet_hours_active: false,
            timezone_validated: true,
        };
        let attempts = notification
            .plan_deliveries(&preferences, None, at())
            .unwrap();
        assert!(
            notification
                .plan_deliveries(&preferences, None, at())
                .unwrap()
                .is_empty()
        );
        let email = attempts
            .iter()
            .find(|value| value.channel == NotificationChannel::Email)
            .unwrap();
        notification
            .record_provider_acceptance(
                &email.dedupe_key,
                "provider-message-1".into(),
                at() + Duration::seconds(1),
            )
            .unwrap();
        let callback = DeliveryCallback {
            callback_id: "callback-1".into(),
            provider_message_ref: "provider-message-1".into(),
            sequence: 1,
            state: DeliveryState::Delivered,
            error_code: None,
            verified: true,
            occurred_at: at() + Duration::seconds(2),
        };
        assert!(
            notification
                .reconcile_callback(callback.clone(), at() + Duration::seconds(2))
                .unwrap()
        );
        assert!(
            !notification
                .reconcile_callback(callback, at() + Duration::seconds(3))
                .unwrap()
        );
        notification.acknowledge("recipient", at()).unwrap();
        assert_eq!(notification.inbox_state, InboxState::Acknowledged);
    }
}
