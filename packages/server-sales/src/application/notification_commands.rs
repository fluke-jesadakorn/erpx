//! Native Sales customer-notification settings/queue/retry commands.
use crate::domain::notifications::{
    DeliveryFacts, NotificationActor, NotificationReceipt, NotificationReceiptSnapshot,
    NoticeSource, QueueNotification, RetryNotification, SettingsReceipt, UpdateSettings,
    access_changed, already_notified, consent_required, delivery_not_found, feature_unavailable,
    idempotency_conflict, not_failed, notification_not_found, provider_required, readback_invalid,
    replay_notice, replay_settings, source_changed, version_conflict,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;

pub type NotificationFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationPurpose {
    NewWork,
    SettleExisting,
    History,
}

#[derive(Clone, Debug)]
pub struct SettingsPlan<'a> {
    pub actor: &'a NotificationActor,
    pub key: &'a str,
    pub input: &'a UpdateSettings,
    pub receipt: &'a SettingsReceipt,
    pub request_hash: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct QueuePlan<'a> {
    pub actor: &'a NotificationActor,
    pub key: &'a str,
    pub fulfillment_id: &'a str,
    pub input: &'a QueueNotification,
    pub facts: &'a DeliveryFacts,
    pub id: &'a str,
    pub recipient_email: &'a str,
    pub subject: &'a str,
    pub body: &'a str,
    pub receipt: &'a NotificationReceipt,
    pub request_hash: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct RetryPlan<'a> {
    pub actor: &'a NotificationActor,
    pub key: &'a str,
    pub input: &'a RetryNotification,
    pub source: &'a NoticeSource,
    pub receipt: &'a NotificationReceipt,
    pub request_hash: &'a str,
    pub now: i64,
}

pub trait NotificationCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> NotificationFuture<'a, (), Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> NotificationFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> NotificationFuture<'a, Option<NotificationReceiptSnapshot>, Self::Error>;
    fn settings<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> NotificationFuture<'a, Option<(String, i64)>, Self::Error>;
    fn delivery<'a>(
        &'a mut self,
        actor: &'a NotificationActor,
        fulfillment_id: &'a str,
    ) -> NotificationFuture<'a, Option<DeliveryFacts>, Self::Error>;
    fn delivery_lines<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> NotificationFuture<'a, Vec<(String, i64, String)>, Self::Error>;
    fn existing<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> NotificationFuture<'a, Option<String>, Self::Error>;
    fn notice<'a>(
        &'a mut self,
        id: &'a str,
    ) -> NotificationFuture<'a, Option<NoticeSource>, Self::Error>;
    fn feature_ready<'a>(
        &'a mut self,
        company_id: &'a str,
        now: i64,
    ) -> NotificationFuture<'a, bool, Self::Error>;
    fn capability<'a>(
        &'a mut self,
        company_id: &'a str,
        purpose: NotificationPurpose,
        now: i64,
    ) -> NotificationFuture<'a, (), Self::Error>;
    fn current_authority<'a>(
        &'a mut self,
        actor: &'a NotificationActor,
        permission: &'a str,
        now: i64,
    ) -> NotificationFuture<'a, bool, Self::Error>;
    fn commit_settings<'a>(
        &'a mut self,
        plan: &'a SettingsPlan<'a>,
        now: i64,
    ) -> NotificationFuture<'a, (), Self::Error>;
    fn commit_queue<'a>(
        &'a mut self,
        plan: &'a QueuePlan<'a>,
        now: i64,
    ) -> NotificationFuture<'a, (), Self::Error>;
    fn commit_retry<'a>(
        &'a mut self,
        plan: &'a RetryPlan<'a>,
        now: i64,
    ) -> NotificationFuture<'a, (), Self::Error>;
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_key(key: &str) -> Result<(), BusinessError> {
    if key.is_empty() || key.len() > 200 || key.chars().any(char::is_control) {
        return Err(crate::domain::notifications::request_invalid());
    }
    Ok(())
}

fn require(actor: &NotificationActor, permission: &str) -> Result<(), BusinessError> {
    if !actor.can(permission) {
        return Err(access_changed());
    }
    Ok(())
}

fn prior_key(
    snapshot: &NotificationReceiptSnapshot,
    hash: &str,
    operation: &str,
    record_id: &str,
    actor: &NotificationActor,
) -> Result<(), BusinessError> {
    if snapshot.request_hash != hash
        || snapshot.operation != operation
        || snapshot.record_id != record_id
        || snapshot.actor_id != actor.user_id
    {
        return Err(idempotency_conflict());
    }
    Ok(())
}

pub async fn update_settings<R: NotificationCommandRepository>(
    actor: &NotificationActor,
    raw: UpdateSettings,
    key: &str,
    repository: &mut R,
    now: i64,
) -> Result<SettingsReceipt, R::Error> {
    validate_key(key)?;
    require(actor, "commerce.approve")?;
    let input = raw.validate()?;
    repository.lock_command(key).await?;
    let identity = input.identity(actor, key);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if !repository.feature_ready(&actor.company_id, now).await? {
        return Err(feature_unavailable().into());
    }
    let prior = repository.receipt(key).await?;
    if let Some(snapshot) = prior.as_ref() {
        prior_key(snapshot, &hash, crate::domain::notifications::SETTINGS_OPERATION, &actor.company_id, actor)?;
        repository
            .capability(&actor.company_id, NotificationPurpose::History, now)
            .await?;
        if !repository
            .current_authority(actor, "commerce.approve", now)
            .await?
        {
            return Err(access_changed().into());
        }
        return Ok(replay_settings(snapshot)?);
    }
    let current = repository.settings(&actor.company_id).await?;
    if current.as_ref().map(|(_, version)| *version).unwrap_or(0) != input.expected_version {
        return Err(version_conflict().into());
    }
    let purpose = if input.adapter == crate::domain::notifications::SettingsAdapter::Disabled {
        NotificationPurpose::SettleExisting
    } else {
        NotificationPurpose::NewWork
    };
    repository
        .capability(&actor.company_id, purpose, now)
        .await?;
    let receipt = SettingsReceipt {
        company_id: actor.company_id.clone(),
        adapter: input.adapter.as_str().to_owned(),
        version: input.expected_version + 1,
    };
    repository
        .commit_settings(
            &SettingsPlan {
                actor,
                key,
                input: &input,
                receipt: &receipt,
                request_hash: &hash,
                now,
            },
            now,
        )
        .await?;
    Ok(receipt)
}

pub async fn queue_notification<R: NotificationCommandRepository>(
    actor: &NotificationActor,
    fulfillment_id: &str,
    raw: QueueNotification,
    key: &str,
    repository: &mut R,
    now: i64,
    id: &str,
) -> Result<NotificationReceipt, R::Error> {
    validate_key(key)?;
    require(actor, "commerce.sales")?;
    let input = raw.validate()?;
    repository.lock_command(key).await?;
    let identity = input.identity(actor, fulfillment_id, key);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if !repository.feature_ready(&actor.company_id, now).await? {
        return Err(feature_unavailable().into());
    }
    let prior = repository.receipt(key).await?;
    if let Some(snapshot) = prior.as_ref() {
        prior_key(snapshot, &hash, crate::domain::notifications::QUEUE_OPERATION, fulfillment_id, actor)?;
        repository
            .capability(&actor.company_id, NotificationPurpose::History, now)
            .await?;
        if !repository
            .current_authority(actor, "commerce.sales", now)
            .await?
        {
            return Err(access_changed().into());
        }
        return Ok(replay_notice(snapshot)?);
    }
    let facts = repository
        .delivery(actor, fulfillment_id)
        .await?
        .ok_or_else(delivery_not_found)?;
    if facts.fulfillment_version != input.expected_version {
        return Err(version_conflict().into());
    }
    if facts.notification_consent.as_deref() != Some("granted")
        || facts
            .notification_email
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty)
    {
        return Err(consent_required().into());
    }
    if !matches!(facts.adapter.as_deref(), Some("sandbox" | "resend")) {
        return Err(provider_required().into());
    }
    if repository.existing(fulfillment_id).await?.is_some() {
        return Err(already_notified().into());
    }
    let lines = repository.delivery_lines(fulfillment_id).await?;
    let (subject, body) = crate::domain::notifications::message(&facts, &lines, &input.note);
    repository
        .capability(&actor.company_id, NotificationPurpose::NewWork, now)
        .await?;
    let recipient = facts
        .notification_email
        .as_deref()
        .ok_or_else(consent_required)?
        .to_lowercase();
    let receipt = NotificationReceipt {
        id: id.to_owned(),
        order_id: facts.order_id.clone(),
        status: "pending".into(),
        version: 1,
    };
    repository
        .commit_queue(
            &QueuePlan {
                actor,
                key,
                fulfillment_id,
                input: &input,
                facts: &facts,
                id,
                recipient_email: &recipient,
                subject: &subject,
                body: &body,
                receipt: &receipt,
                request_hash: &hash,
                now,
            },
            now,
        )
        .await?;
    Ok(receipt)
}

pub async fn retry_notification<R: NotificationCommandRepository>(
    actor: &NotificationActor,
    id: &str,
    raw: RetryNotification,
    key: &str,
    repository: &mut R,
    now: i64,
) -> Result<NotificationReceipt, R::Error> {
    validate_key(key)?;
    require(actor, "commerce.sales")?;
    let input = raw.validate()?;
    repository.lock_command(key).await?;
    let identity = input.identity(actor, id, key);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if !repository.feature_ready(&actor.company_id, now).await? {
        return Err(feature_unavailable().into());
    }
    let prior = repository.receipt(key).await?;
    if let Some(snapshot) = prior.as_ref() {
        prior_key(snapshot, &hash, crate::domain::notifications::RETRY_OPERATION, id, actor)?;
        repository
            .capability(&actor.company_id, NotificationPurpose::History, now)
            .await?;
        if !repository
            .current_authority(actor, "commerce.sales", now)
            .await?
        {
            return Err(access_changed().into());
        }
        return Ok(replay_notice(snapshot)?);
    }
    let source = repository
        .notice(id)
        .await?
        .filter(|source| source.in_scope(actor))
        .ok_or_else(notification_not_found)?;
    if source.status != "failed" {
        return Err(not_failed().into());
    }
    if source.version != input.expected_version {
        return Err(version_conflict().into());
    }
    if source.invalid_reason().is_some() {
        return Err(source_changed().into());
    }
    repository
        .capability(&actor.company_id, NotificationPurpose::SettleExisting, now)
        .await?;
    let receipt = NotificationReceipt {
        id: id.to_owned(),
        order_id: source.order_id.clone(),
        status: "pending".into(),
        version: input.expected_version + 1,
    };
    repository
        .commit_retry(
            &RetryPlan {
                actor,
                key,
                input: &input,
                source: &source,
                receipt: &receipt,
                request_hash: &hash,
                now,
            },
            now,
        )
        .await?;
    Ok(receipt)
}
