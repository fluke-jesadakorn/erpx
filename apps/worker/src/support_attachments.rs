//! Bounded support attachment expiry and SLA notification worker seams.
//!
//! The attachment handler is wired to the real control-plane repository by
//! the worker composition root. SLA delivery is fail-closed until an explicit
//! publisher is composed, and successful delivery is acknowledged through the
//! existing durable notification store.

use std::collections::HashSet;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_domain::{
    access::Scope,
    contracts::TenantScope,
    support::{
        MAX_SUPPORT_ATTACHMENT_CONTENT_TYPE_LENGTH, MAX_SUPPORT_ATTACHMENT_LIFETIME,
        MAX_SUPPORT_ATTACHMENT_OBJECT_REF_LENGTH, MAX_SUPPORT_ATTACHMENT_SIZE_BYTES,
        MAX_SUPPORT_ATTACHMENT_UPLOADER_REF_LENGTH, MAX_TICKET_ASSIGNMENT_SCOPE_IDS,
        SupportCapability, SupportCapabilityBinding, SupportCapabilitySecret, SupportMode,
        SupportSlaNotification,
    },
};
use erpx_infrastructure::{
    MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE, PersistedSupportAttachment, SupportPersistence,
    SupportPersistenceError,
};
use thiserror::Error;
use uuid::Uuid;

use super::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const SUPPORT_ATTACHMENT_EXPIRY_HANDLER_NAME: &str = "support_attachment_expiry";
pub const SUPPORT_SLA_NOTIFICATION_HANDLER_NAME: &str = "support_sla_notifications";
pub const SUPPORT_ATTACHMENT_OBJECT_READ_PERMISSION: &str = "support.read";
pub const SUPPORT_ATTACHMENT_MODULE: &str = "support";
const SUPPORT_ATTACHMENT_MAX_AUTHORIZATION_AGE: Duration = Duration::minutes(5);
pub const SUPPORT_ATTACHMENT_MAX_MFA_AGE: Duration = Duration::minutes(5);
const SUPPORT_SLA_DETERMINISTIC_CLAIMANT_ID: Uuid = Uuid::from_u128(1);

type SupportSlaClaimRecord = (SupportSlaNotification, Uuid, DateTime<Utc>, DateTime<Utc>);

#[async_trait]
pub trait SupportAttachmentExpiryStore: Send + Sync + Clone + 'static {
    async fn sweep_expired_attachments(
        &self,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<PersistedSupportAttachment>, SupportPersistenceError>;
}

#[async_trait]
impl SupportAttachmentExpiryStore for SupportPersistence {
    async fn sweep_expired_attachments(
        &self,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<PersistedSupportAttachment>, SupportPersistenceError> {
        SupportPersistence::sweep_expired_attachments(self, at, batch_size).await
    }
}

#[derive(Debug, Clone)]
pub struct SupportAttachmentExpiryHandler<S = SupportPersistence> {
    store: S,
    batch_size: u32,
}

impl<S> SupportAttachmentExpiryHandler<S> {
    pub fn new(store: S, batch_size: u32) -> Self {
        Self {
            store,
            batch_size: batch_size.clamp(1, MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE),
        }
    }

    pub const fn batch_size(&self) -> u32 {
        self.batch_size
    }

    /// Run an expiry sweep against an injected clock. The store is the
    /// authority for the state transition; the worker refuses to report a
    /// row as processed unless the returned record proves that it crossed the
    /// half-open expiry boundary and is terminally `expired`.
    pub async fn run_once_at(
        &self,
        at: DateTime<Utc>,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError>
    where
        S: SupportAttachmentExpiryStore,
    {
        let expired = self
            .store
            .sweep_expired_attachments(at, self.batch_size)
            .await
            .map_err(redacted_attachment_error)?;
        if expired.len() > self.batch_size as usize {
            return Err(WorkerRuntimeError::Configuration(
                "support attachment expiry store exceeded its batch bound".into(),
            ));
        }
        validate_expired_attachment_batch(&expired, at)?;
        let processed = expired.len() as u64;
        Ok(WorkerHandlerReport {
            processed,
            has_work: processed != 0,
            ..WorkerHandlerReport::default()
        })
    }

    /// Authorize a single object read only at the exact persisted attachment
    /// boundary. The caller must supply the binding that was assembled at the
    /// authenticated request boundary; capability authentication then checks
    /// that every immutable field matches the persisted capability.
    ///
    /// `authorization_checked_at` is deliberately bounded to five minutes so
    /// a caller cannot keep using a stale authorization snapshot after a
    /// capability/session revocation. The database-backed capability lookup
    /// remains the source of truth for the next request.
    #[allow(clippy::too_many_arguments)]
    pub fn authorize_object_access(
        attachment: &PersistedSupportAttachment,
        capability: &SupportCapability,
        capability_secret: &SupportCapabilitySecret,
        capability_binding: &SupportCapabilityBinding,
        requested_object_ref: &str,
        requested_ticket_id: Uuid,
        requested_customer_account_id: Uuid,
        requested_scope: &TenantScope,
        authorization_checked_at: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<(), WorkerRuntimeError> {
        validate_attachment_metadata(attachment)?;
        if requested_ticket_id.is_nil() || requested_customer_account_id.is_nil() {
            return Err(object_access_denied());
        }
        validate_object_ref(requested_object_ref)?;
        if requested_object_ref != attachment.value.object_ref {
            return Err(object_access_denied());
        }
        if attachment.status != "active"
            || !attachment.value.is_active_at(at)
            || attachment.value.expires_at <= at
        {
            return Err(object_access_denied());
        }
        if attachment.value.ticket_id != requested_ticket_id
            || attachment.value.customer_account_id != requested_customer_account_id
            || attachment.value.scope != *requested_scope
        {
            return Err(object_access_denied());
        }
        if !timestamp_is_fresh(
            authorization_checked_at,
            at,
            SUPPORT_ATTACHMENT_MAX_AUTHORIZATION_AGE,
        ) {
            return Err(object_access_denied());
        }
        if capability.binding.module != SUPPORT_ATTACHMENT_MODULE
            || capability.binding.mode != SupportMode::DiagnosticRead
            || !capability
                .binding
                .permissions
                .iter()
                .any(|permission| permission == SUPPORT_ATTACHMENT_OBJECT_READ_PERMISSION)
            || capability.binding.ticket_id != requested_ticket_id
            || capability.binding.customer_account_id != requested_customer_account_id
        {
            return Err(object_access_denied());
        }
        let capability_scope = parse_canonical_scope(&capability.binding.scope)?;
        let attachment_scope = tenant_scope_as_access_scope(requested_scope)?;
        if !capability_scope.contains(&attachment_scope) {
            return Err(object_access_denied());
        }
        capability
            .authenticate(capability_secret, capability_binding, at)
            .map_err(|_| object_access_denied())
    }

    /// High-risk support reads require a separately supplied, recent MFA
    /// assertion. Customer approval and a bearer capability are necessary but
    /// are not substitutes for the operator's fresh MFA proof.
    #[allow(clippy::too_many_arguments)]
    pub fn authorize_high_risk_object_access(
        attachment: &PersistedSupportAttachment,
        capability: &SupportCapability,
        capability_secret: &SupportCapabilitySecret,
        capability_binding: &SupportCapabilityBinding,
        requested_object_ref: &str,
        requested_ticket_id: Uuid,
        requested_customer_account_id: Uuid,
        requested_scope: &TenantScope,
        authorization_checked_at: DateTime<Utc>,
        mfa_evidence_id: Uuid,
        mfa_verified_at: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<(), WorkerRuntimeError> {
        Self::authorize_object_access(
            attachment,
            capability,
            capability_secret,
            capability_binding,
            requested_object_ref,
            requested_ticket_id,
            requested_customer_account_id,
            requested_scope,
            authorization_checked_at,
            at,
        )?;
        if mfa_evidence_id.is_nil()
            || !timestamp_is_fresh(mfa_verified_at, at, SUPPORT_ATTACHMENT_MAX_MFA_AGE)
        {
            return Err(object_access_denied());
        }
        Ok(())
    }
}

#[async_trait]
impl<S> NamedWorkerHandler for SupportAttachmentExpiryHandler<S>
where
    S: SupportAttachmentExpiryStore,
{
    fn name(&self) -> &str {
        SUPPORT_ATTACHMENT_EXPIRY_HANDLER_NAME
    }

    async fn run_once(&self, _worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        self.run_once_at(Utc::now()).await
    }
}

#[async_trait]
pub trait SupportSlaNotificationSource: Send + Sync + Clone + 'static {
    /// The source must durably bind this claimant to a unique lease token.
    /// Legacy key-only acknowledgement is intentionally not an acceptable
    /// delivery boundary because a previous worker can replay the same key
    /// after another worker has reclaimed it.
    fn supports_durable_claim_identity(&self) -> bool {
        false
    }

    async fn claim_due_sla_notifications(
        &self,
        claimant_id: Uuid,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<SupportSlaClaimRecord>, SupportPersistenceError>;

    /// Acknowledgement is separate from claim and must match the durable
    /// claimant, token, original claim timestamp, and lease returned by the
    /// source. A short/expired or replayed lease therefore remains retryable
    /// without a compensating delete.
    async fn acknowledge_sla_notifications(
        &self,
        claimant_id: Uuid,
        claims: &[SupportSlaClaimRecord],
        at: DateTime<Utc>,
    ) -> Result<u64, SupportPersistenceError> {
        let _ = (claimant_id, claims, at);
        Err(SupportPersistenceError::InvalidData(
            "SLA notification durable claim acknowledgement boundary is not configured".into(),
        ))
    }

    /// Provider/channel-aware acknowledgement. The default keeps custom
    /// sources compatible while the real SQLx source overrides it with an
    /// append-only delivery event.
    async fn acknowledge_sla_notifications_with_route(
        &self,
        claimant_id: Uuid,
        claims: &[SupportSlaClaimRecord],
        at: DateTime<Utc>,
        provider_ref: &str,
        channel: &str,
    ) -> Result<u64, SupportPersistenceError> {
        let _ = (provider_ref, channel);
        self.acknowledge_sla_notifications(claimant_id, claims, at)
            .await
    }
}

#[async_trait]
impl SupportSlaNotificationSource for SupportPersistence {
    fn supports_durable_claim_identity(&self) -> bool {
        true
    }

    async fn claim_due_sla_notifications(
        &self,
        claimant_id: Uuid,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<SupportSlaClaimRecord>, SupportPersistenceError> {
        self.claim_due_sla_notifications_for_worker(claimant_id, at, batch_size)
            .await
    }

    async fn acknowledge_sla_notifications(
        &self,
        claimant_id: Uuid,
        claims: &[SupportSlaClaimRecord],
        at: DateTime<Utc>,
    ) -> Result<u64, SupportPersistenceError> {
        self.mark_sla_notifications_delivered_for_worker(claimant_id, claims, at)
            .await
    }

    async fn acknowledge_sla_notifications_with_route(
        &self,
        claimant_id: Uuid,
        claims: &[SupportSlaClaimRecord],
        at: DateTime<Utc>,
        provider_ref: &str,
        channel: &str,
    ) -> Result<u64, SupportPersistenceError> {
        self.mark_sla_notifications_delivered_for_worker_with_route(
            claimant_id,
            claims,
            at,
            provider_ref,
            channel,
        )
        .await
    }
}

#[derive(Debug, Clone)]
pub struct SupportSlaNotificationHandler<S, P = UnconfiguredSlaNotificationPublisher> {
    source: S,
    publisher: P,
    batch_size: u32,
}

impl<S> SupportSlaNotificationHandler<S> {
    pub fn new(source: S, batch_size: u32) -> Self {
        Self {
            source,
            publisher: UnconfiguredSlaNotificationPublisher,
            batch_size: batch_size.clamp(1, MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE),
        }
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum SupportSlaNotificationPublishError {
    #[error("SLA notification publisher is unavailable")]
    Unavailable,
    #[error("SLA notification publisher rejected the notification")]
    Rejected,
}

/// Non-secret provider/channel identity required before the worker claims a
/// durable SLA notification. Credentials and provider response bodies remain
/// outside this contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportSlaNotificationRoute {
    provider_ref: String,
    channel: String,
}

impl SupportSlaNotificationRoute {
    pub fn new(
        provider_ref: impl Into<String>,
        channel: impl Into<String>,
    ) -> Result<Self, WorkerRuntimeError> {
        let provider_ref = provider_ref.into();
        let channel = channel.into();
        validate_delivery_route_component(&provider_ref, "provider")?;
        validate_delivery_route_component(&channel, "channel")?;
        Ok(Self {
            provider_ref,
            channel,
        })
    }

    pub fn provider_ref(&self) -> &str {
        &self.provider_ref
    }

    pub fn channel(&self) -> &str {
        &self.channel
    }
}

#[async_trait]
pub trait SupportSlaNotificationPublisher: Send + Sync + Clone + 'static {
    fn is_configured(&self) -> bool {
        false
    }

    /// Return the non-secret route that accepted delivery. A configured
    /// publisher without an explicit route is not a usable provider boundary.
    fn delivery_route(&self) -> Option<SupportSlaNotificationRoute> {
        None
    }

    async fn publish(
        &self,
        notification: &SupportSlaNotification,
    ) -> Result<(), SupportSlaNotificationPublishError>;
}

#[derive(Debug, Clone, Copy)]
pub struct UnconfiguredSlaNotificationPublisher;

#[async_trait]
impl SupportSlaNotificationPublisher for UnconfiguredSlaNotificationPublisher {
    fn is_configured(&self) -> bool {
        false
    }

    async fn publish(
        &self,
        _notification: &SupportSlaNotification,
    ) -> Result<(), SupportSlaNotificationPublishError> {
        Err(SupportSlaNotificationPublishError::Unavailable)
    }
}

impl<S, P> SupportSlaNotificationHandler<S, P>
where
    S: SupportSlaNotificationSource,
    P: SupportSlaNotificationPublisher,
{
    pub fn new_with_publisher(
        source: S,
        publisher: P,
        batch_size: u32,
    ) -> Result<Self, WorkerRuntimeError> {
        if batch_size == 0 || batch_size > MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE {
            return Err(WorkerRuntimeError::Configuration(
                "support SLA notification batch size is outside its bound".into(),
            ));
        }
        Ok(Self {
            source,
            publisher,
            batch_size,
        })
    }
}

#[async_trait]
impl<S, P> NamedWorkerHandler for SupportSlaNotificationHandler<S, P>
where
    S: SupportSlaNotificationSource,
    P: SupportSlaNotificationPublisher,
{
    fn name(&self) -> &str {
        SUPPORT_SLA_NOTIFICATION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        self.run_once_live(worker_id).await
    }
}

impl<S, P> SupportSlaNotificationHandler<S, P>
where
    S: SupportSlaNotificationSource,
    P: SupportSlaNotificationPublisher,
{
    /// Deterministic form used by focused tests and replay tooling. Production
    /// workers use `run_once_live`, which refreshes the acknowledgement clock
    /// after provider I/O so an expired claim cannot be acknowledged.
    pub async fn run_once_at(
        &self,
        at: DateTime<Utc>,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        self.run_once_at_for_worker(SUPPORT_SLA_DETERMINISTIC_CLAIMANT_ID, at)
            .await
    }

    pub async fn run_once_at_for_worker(
        &self,
        worker_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        self.run_once_with_times(worker_id, at, at).await
    }

    async fn run_once_live(
        &self,
        worker_id: Uuid,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let at = Utc::now();
        self.run_once_with_dynamic_ack_time(worker_id, at).await
    }

    async fn run_once_with_dynamic_ack_time(
        &self,
        worker_id: Uuid,
        claim_at: DateTime<Utc>,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let route = self.configured_route()?;
        if worker_id.is_nil() || !self.source.supports_durable_claim_identity() {
            return Err(WorkerRuntimeError::Configuration(
                "support SLA durable claim identity boundary is not configured".into(),
            ));
        }
        let notifications = self
            .source
            .claim_due_sla_notifications(worker_id, claim_at, self.batch_size)
            .await
            .map_err(redacted_attachment_error)?;
        self.validate_claimed_notifications(&notifications, worker_id, claim_at)?;
        if notifications.len() > self.batch_size as usize {
            return Err(WorkerRuntimeError::Configuration(
                "support SLA notification source exceeded its batch bound".into(),
            ));
        }
        if notifications.is_empty() {
            return Ok(WorkerHandlerReport::default());
        }

        let claimed = notifications.len() as u64;
        let mut delivered = Vec::with_capacity(notifications.len());
        let mut retried = 0_u64;
        for (index, claim) in notifications.iter().enumerate() {
            let notification = &claim.0;
            if claim_at >= notification.expires_at {
                return Err(WorkerRuntimeError::Configuration(
                    "support SLA notification source returned an expired row".into(),
                ));
            }
            if !notification_is_within_window(notification, claim_at) {
                retried += 1;
                continue;
            }
            match self.publisher.publish(notification).await {
                Ok(()) => delivered.push(claim.clone()),
                Err(_) => {
                    retried += (notifications.len() - index) as u64;
                    break;
                }
            }
        }
        let acknowledged = if delivered.is_empty() {
            0
        } else {
            let ack_at = Utc::now();
            let live_claims = delivered
                .iter()
                .filter(|claim| claim_lease_is_live(claim, ack_at))
                .cloned()
                .collect::<Vec<_>>();
            if live_claims.is_empty() {
                return self.report_sla_delivery(claimed, delivered.len() as u64, 0, retried);
            }
            let live_claim_count = live_claims.len() as u64;
            let acknowledged = self
                .source
                .acknowledge_sla_notifications_with_route(
                    worker_id,
                    &live_claims,
                    ack_at,
                    route.provider_ref(),
                    route.channel(),
                )
                .await
                .map_err(redacted_attachment_error)?;
            validate_sla_acknowledgement_count(acknowledged, live_claim_count)?;
            acknowledged
        };
        self.report_sla_delivery(claimed, delivered.len() as u64, acknowledged, retried)
    }

    async fn run_once_with_times(
        &self,
        worker_id: Uuid,
        at: DateTime<Utc>,
        acknowledgement_at: DateTime<Utc>,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let route = self.configured_route()?;
        if worker_id.is_nil() || !self.source.supports_durable_claim_identity() {
            return Err(WorkerRuntimeError::Configuration(
                "support SLA durable claim identity boundary is not configured".into(),
            ));
        }
        let notifications = self
            .source
            .claim_due_sla_notifications(worker_id, at, self.batch_size)
            .await
            .map_err(redacted_attachment_error)?;
        self.validate_claimed_notifications(&notifications, worker_id, at)?;
        if notifications.len() > self.batch_size as usize {
            return Err(WorkerRuntimeError::Configuration(
                "support SLA notification source exceeded its batch bound".into(),
            ));
        }
        if notifications.is_empty() {
            return Ok(WorkerHandlerReport::default());
        }

        let claimed = notifications.len() as u64;
        let mut delivered = Vec::with_capacity(notifications.len());
        let mut retried = 0_u64;
        for (index, claim) in notifications.iter().enumerate() {
            let notification = &claim.0;
            if at >= notification.expires_at {
                return Err(WorkerRuntimeError::Configuration(
                    "support SLA notification source returned an expired row".into(),
                ));
            }
            if !notification_is_within_window(notification, at) {
                retried += 1;
                continue;
            }
            match self.publisher.publish(notification).await {
                Ok(()) => delivered.push(claim.clone()),
                Err(_) => {
                    retried += (notifications.len() - index) as u64;
                    break;
                }
            }
        }
        let acknowledged = if delivered.is_empty() {
            0
        } else {
            let live_claims = delivered
                .iter()
                .filter(|claim| claim_lease_is_live(claim, acknowledgement_at))
                .cloned()
                .collect::<Vec<_>>();
            if live_claims.is_empty() {
                return self.report_sla_delivery(claimed, delivered.len() as u64, 0, retried);
            }
            let live_claim_count = live_claims.len() as u64;
            let acknowledged = self
                .source
                .acknowledge_sla_notifications_with_route(
                    worker_id,
                    &live_claims,
                    acknowledgement_at,
                    route.provider_ref(),
                    route.channel(),
                )
                .await
                .map_err(redacted_attachment_error)?;
            validate_sla_acknowledgement_count(acknowledged, live_claim_count)?;
            acknowledged
        };
        self.report_sla_delivery(claimed, delivered.len() as u64, acknowledged, retried)
    }

    fn configured_route(&self) -> Result<SupportSlaNotificationRoute, WorkerRuntimeError> {
        if !self.publisher.is_configured() {
            return Err(WorkerRuntimeError::Configuration(
                "support SLA notification delivery boundary is not configured".into(),
            ));
        }
        self.publisher.delivery_route().ok_or_else(|| {
            WorkerRuntimeError::Configuration(
                "support SLA provider/channel route is not configured".into(),
            )
        })
    }

    fn validate_claimed_notifications(
        &self,
        notifications: &[SupportSlaClaimRecord],
        claimant_id: Uuid,
        claim_at: DateTime<Utc>,
    ) -> Result<(), WorkerRuntimeError> {
        let mut keys = HashSet::with_capacity(notifications.len());
        let mut claim_tokens = HashSet::with_capacity(notifications.len());
        for claim in notifications {
            validate_sla_claim(claim, claimant_id, claim_at)?;
            let notification = &claim.0;
            validate_sla_notification(notification)?;
            if !keys.insert(notification.key.as_str()) {
                return Err(WorkerRuntimeError::Configuration(
                    "support SLA notification source returned duplicate keys".into(),
                ));
            }
            if !claim_tokens.insert(claim.1) {
                return Err(WorkerRuntimeError::Configuration(
                    "support SLA notification source returned duplicate claim tokens".into(),
                ));
            }
        }
        Ok(())
    }

    fn report_sla_delivery(
        &self,
        claimed: u64,
        published: u64,
        acknowledged: u64,
        mut retried: u64,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if acknowledged > published {
            return Err(WorkerRuntimeError::Configuration(
                "support SLA acknowledgement exceeded the delivered batch".into(),
            ));
        }
        retried += published - acknowledged;
        Ok(WorkerHandlerReport {
            claimed,
            processed: acknowledged,
            published,
            retried,
            has_work: true,
            ..WorkerHandlerReport::default()
        })
    }
}

fn validate_expired_attachment(
    attachment: &PersistedSupportAttachment,
    at: DateTime<Utc>,
) -> Result<(), WorkerRuntimeError> {
    validate_attachment_metadata(attachment)?;
    if attachment.status != "expired"
        || attachment.version <= 1
        || attachment.value.expires_at > at
        || attachment.value.is_active_at(at)
        || !is_valid_object_ref(&attachment.value.object_ref)
    {
        return Err(WorkerRuntimeError::Configuration(
            "support attachment expiry store returned a non-terminal attachment".into(),
        ));
    }
    Ok(())
}

/// Reconcile the worker result with the contract of the authoritative SQLx
/// sweep before reporting scheduler progress. `SupportPersistence` selects
/// rows with `ORDER BY expires_at, id`, and each locked row can transition only
/// once from `active` to terminal `expired`. A duplicate or out-of-order row
/// therefore proves that the returned batch is not a faithful representation
/// of the committed state transition; fail closed so the scheduler does not
/// acknowledge an unverifiable batch.
fn validate_expired_attachment_batch(
    attachments: &[PersistedSupportAttachment],
    at: DateTime<Utc>,
) -> Result<(), WorkerRuntimeError> {
    let mut identities = HashSet::with_capacity(attachments.len());
    let mut previous_key = None;
    for attachment in attachments {
        validate_expired_attachment(attachment, at)?;
        if !identities.insert(attachment.value.id) {
            return Err(WorkerRuntimeError::Configuration(
                "support attachment expiry store returned duplicate attachment identity".into(),
            ));
        }
        let key = (attachment.value.expires_at, attachment.value.id);
        if previous_key.is_some_and(|previous| key < previous) {
            return Err(WorkerRuntimeError::Configuration(
                "support attachment expiry store returned an out-of-order batch".into(),
            ));
        }
        previous_key = Some(key);
    }
    Ok(())
}

fn validate_attachment_metadata(
    attachment: &PersistedSupportAttachment,
) -> Result<(), WorkerRuntimeError> {
    let value = &attachment.value;
    if value.id.is_nil()
        || value.ticket_id.is_nil()
        || value.customer_account_id.is_nil()
        || !matches!(attachment.status.as_str(), "active" | "revoked" | "expired")
        || attachment.version == 0
        || attachment.ticket_version == 0
        || !is_valid_object_ref(&value.object_ref)
        || value.content_size_bytes == 0
        || value.content_size_bytes > MAX_SUPPORT_ATTACHMENT_SIZE_BYTES
        || value.sha256_hex.len() != 64
        || !value
            .sha256_hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || value.sha256_hex != value.sha256_hex.to_ascii_lowercase()
        || !is_valid_content_type(&value.content_type)
        || value.uploader_ref.trim().is_empty()
        || value.uploader_ref.len() > MAX_SUPPORT_ATTACHMENT_UPLOADER_REF_LENGTH
        || value.uploader_ref.chars().any(char::is_control)
        || value.expires_at <= value.uploaded_at
        || value.expires_at > value.uploaded_at + MAX_SUPPORT_ATTACHMENT_LIFETIME
    {
        return Err(WorkerRuntimeError::Configuration(
            "support attachment metadata is invalid".into(),
        ));
    }
    tenant_scope_as_access_scope(&value.scope)?;
    Ok(())
}

fn is_valid_content_type(value: &str) -> bool {
    value.len() <= MAX_SUPPORT_ATTACHMENT_CONTENT_TYPE_LENGTH
        && value.split('/').count() == 2
        && value.split('/').all(|part| !part.is_empty())
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-' | b'/'
                )
        })
}

fn validate_object_ref(value: &str) -> Result<(), WorkerRuntimeError> {
    if is_valid_object_ref(value) {
        Ok(())
    } else {
        Err(object_access_denied())
    }
}

fn is_valid_object_ref(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SUPPORT_ATTACHMENT_OBJECT_REF_LENGTH
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains("//")
        && !value.contains("http:")
        && !value.contains("https:")
        && value.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
}

fn tenant_scope_as_access_scope(scope: &TenantScope) -> Result<Scope, WorkerRuntimeError> {
    let converted = match scope {
        TenantScope::Platform => Scope::Platform,
        TenantScope::Holding { holding_id } if !holding_id.is_nil() => Scope::Holding {
            holding_id: *holding_id,
        },
        TenantScope::Company {
            holding_id,
            company_id,
        } if !holding_id.is_nil() && !company_id.is_nil() => Scope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        },
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } if !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil() => Scope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        },
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } if !holding_id.is_nil()
            && !company_ids.is_empty()
            && company_ids.len() <= MAX_TICKET_ASSIGNMENT_SCOPE_IDS
            && branch_ids.len() <= MAX_TICKET_ASSIGNMENT_SCOPE_IDS
            && company_ids.iter().all(|id| !id.is_nil())
            && branch_ids.iter().all(|id| !id.is_nil()) =>
        {
            Scope::Subtree {
                holding_id: *holding_id,
                company_ids: company_ids.clone(),
                branch_ids: branch_ids.clone(),
            }
        }
        _ => return Err(object_access_denied()),
    };
    Ok(converted)
}

fn parse_canonical_scope(value: &str) -> Result<Scope, WorkerRuntimeError> {
    let parse_uuid = |raw: &str| Uuid::parse_str(raw).map_err(|_| object_access_denied());
    let scope = if value == "platform" {
        Scope::Platform
    } else if let Some(raw) = value.strip_prefix("holding:") {
        Scope::Holding {
            holding_id: parse_uuid(raw)?,
        }
    } else if let Some(raw) = value.strip_prefix("company:") {
        let mut parts = raw.split(':');
        let holding_id = parse_uuid(parts.next().ok_or_else(object_access_denied)?)?;
        let company_id = parse_uuid(parts.next().ok_or_else(object_access_denied)?)?;
        if parts.next().is_some() {
            return Err(object_access_denied());
        }
        Scope::Company {
            holding_id,
            company_id,
        }
    } else if let Some(raw) = value.strip_prefix("branch:") {
        let mut parts = raw.split(':');
        let holding_id = parse_uuid(parts.next().ok_or_else(object_access_denied)?)?;
        let company_id = parse_uuid(parts.next().ok_or_else(object_access_denied)?)?;
        let branch_id = parse_uuid(parts.next().ok_or_else(object_access_denied)?)?;
        if parts.next().is_some() {
            return Err(object_access_denied());
        }
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        }
    } else if let Some(raw) = value.strip_prefix("subtree:") {
        let (holding, rest) = raw
            .split_once(":companies=")
            .ok_or_else(object_access_denied)?;
        let (companies, branches) = rest
            .split_once(":branches=")
            .ok_or_else(object_access_denied)?;
        let holding_id = parse_uuid(holding)?;
        let parse_ids = |raw: &str| -> Result<Vec<Uuid>, WorkerRuntimeError> {
            if raw.is_empty() {
                return Ok(Vec::new());
            }
            raw.split(',').map(parse_uuid).collect()
        };
        let company_ids = parse_ids(companies)?;
        let branch_ids = parse_ids(branches)?;
        if company_ids.is_empty()
            || company_ids.len() > MAX_TICKET_ASSIGNMENT_SCOPE_IDS
            || branch_ids.len() > MAX_TICKET_ASSIGNMENT_SCOPE_IDS
            || company_ids.iter().any(Uuid::is_nil)
            || branch_ids.iter().any(Uuid::is_nil)
        {
            return Err(object_access_denied());
        }
        Scope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        }
    } else {
        return Err(object_access_denied());
    };
    if !is_valid_access_scope(&scope) || scope.canonical_key() != value {
        return Err(object_access_denied());
    }
    Ok(scope)
}

fn is_valid_access_scope(scope: &Scope) -> bool {
    match scope {
        Scope::Platform => true,
        Scope::Holding { holding_id } => !holding_id.is_nil(),
        Scope::Company {
            holding_id,
            company_id,
        } => !holding_id.is_nil() && !company_id.is_nil(),
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil(),
        Scope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => {
            !holding_id.is_nil()
                && !company_ids.is_empty()
                && company_ids.len() <= MAX_TICKET_ASSIGNMENT_SCOPE_IDS
                && branch_ids.len() <= MAX_TICKET_ASSIGNMENT_SCOPE_IDS
                && company_ids.iter().all(|id| !id.is_nil())
                && branch_ids.iter().all(|id| !id.is_nil())
        }
    }
}

fn object_access_denied() -> WorkerRuntimeError {
    WorkerRuntimeError::Configuration("support attachment object access denied".into())
}

fn timestamp_is_fresh(timestamp: DateTime<Utc>, at: DateTime<Utc>, maximum_age: Duration) -> bool {
    maximum_age >= Duration::zero()
        && timestamp <= at
        && at
            .checked_sub_signed(maximum_age)
            .is_some_and(|lower_bound| timestamp >= lower_bound)
}

fn validate_sla_claim(
    claim: &SupportSlaClaimRecord,
    claimant_id: Uuid,
    claim_at: DateTime<Utc>,
) -> Result<(), WorkerRuntimeError> {
    let (_, claim_token, claimed_at, claim_expires_at) = claim;
    if claimant_id.is_nil()
        || claim_token.is_nil()
        || *claimed_at != claim_at
        || *claim_expires_at <= *claimed_at
        || bounded_notification_lease(claim_expires_at.signed_duration_since(*claimed_at)).is_err()
    {
        return Err(WorkerRuntimeError::Configuration(
            "support SLA notification source returned an invalid durable claim".into(),
        ));
    }
    Ok(())
}

fn claim_lease_is_live(claim: &SupportSlaClaimRecord, at: DateTime<Utc>) -> bool {
    let notification = &claim.0;
    let claimed_at = claim.2;
    let claim_expires_at = claim.3;
    claimed_at <= at && at < claim_expires_at && at < notification.expires_at
}

fn validate_sla_acknowledgement_count(
    acknowledged: u64,
    live_claim_count: u64,
) -> Result<(), WorkerRuntimeError> {
    if acknowledged > live_claim_count {
        return Err(WorkerRuntimeError::Configuration(
            "support SLA acknowledgement exceeded the live claimed batch".into(),
        ));
    }
    Ok(())
}

fn validate_sla_notification(
    notification: &SupportSlaNotification,
) -> Result<(), WorkerRuntimeError> {
    if notification.key.trim().is_empty()
        || notification.key.len() > 255
        || notification.key.chars().any(char::is_control)
        || notification.ticket_id.is_nil()
        || notification.customer_account_id.is_nil()
        || notification.not_before > notification.deadline
        || notification.deadline > notification.expires_at
        || notification.not_before >= notification.expires_at
    {
        return Err(WorkerRuntimeError::Configuration(
            "support SLA notification source returned invalid data".into(),
        ));
    }
    Ok(())
}

fn validate_delivery_route_component(value: &str, field: &str) -> Result<(), WorkerRuntimeError> {
    if value.trim().is_empty()
        || value.len() > 128
        || value.chars().any(char::is_control)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(WorkerRuntimeError::Configuration(format!(
            "support SLA {field} route is invalid"
        )));
    }
    Ok(())
}

fn redacted_attachment_error(error: SupportPersistenceError) -> WorkerRuntimeError {
    let kind = match error {
        SupportPersistenceError::Database(_) => "database",
        SupportPersistenceError::Serialization(_) => "serialization",
        SupportPersistenceError::IdempotencyConflict { .. } => "idempotency",
        SupportPersistenceError::NotFound { .. } => "not_found",
        SupportPersistenceError::OptimisticConcurrency { .. } => "concurrency",
        SupportPersistenceError::InvalidData(_) => "invalid_data",
        SupportPersistenceError::AttachmentScopeDenied
        | SupportPersistenceError::AttachmentBindingDenied
        | SupportPersistenceError::AttachmentClosedTicket
        | SupportPersistenceError::AttachmentNotActive
        | SupportPersistenceError::AttachmentNotExpired
        | SupportPersistenceError::AttachmentLimitExceeded => "attachment_policy",
        SupportPersistenceError::AccessRequestTicketBindingDenied => "access_policy",
    };
    tracing::error!(error_kind = kind, "support attachment worker failed");
    WorkerRuntimeError::Configuration("support attachment worker failed".into())
}

pub fn notification_is_within_window(
    notification: &SupportSlaNotification,
    at: DateTime<Utc>,
) -> bool {
    at >= notification.not_before && at < notification.expires_at
}

pub fn bounded_notification_lease(window: Duration) -> Result<Duration, WorkerRuntimeError> {
    if window <= Duration::zero() || window > Duration::days(7) {
        return Err(WorkerRuntimeError::Configuration(
            "SLA notification lease is outside the bounded window".into(),
        ));
    }
    Ok(window)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::support::{SupportNotificationAudience, SupportSlaNotificationClassification};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Clone)]
    struct FakeExpiryStore;

    #[async_trait]
    impl SupportAttachmentExpiryStore for FakeExpiryStore {
        async fn sweep_expired_attachments(
            &self,
            _at: DateTime<Utc>,
            _batch_size: u32,
        ) -> Result<Vec<PersistedSupportAttachment>, SupportPersistenceError> {
            Ok(Vec::new())
        }
    }

    type ActiveSlaClaim = (Uuid, Uuid, DateTime<Utc>, DateTime<Utc>);

    #[derive(Clone, Default)]
    struct FakeSlaSource {
        pending: Arc<Mutex<Vec<SupportSlaNotification>>>,
        acknowledged: Arc<Mutex<Vec<String>>>,
        active: Arc<Mutex<HashMap<String, ActiveSlaClaim>>>,
    }

    #[async_trait]
    impl SupportSlaNotificationSource for FakeSlaSource {
        fn supports_durable_claim_identity(&self) -> bool {
            true
        }

        async fn claim_due_sla_notifications(
            &self,
            claimant_id: Uuid,
            at: DateTime<Utc>,
            _batch_size: u32,
        ) -> Result<Vec<SupportSlaClaimRecord>, SupportPersistenceError> {
            let notifications = std::mem::take(&mut *self.pending.lock().expect("pending mutex"));
            let claim_expires_at = at + Duration::minutes(15);
            let claims = notifications
                .into_iter()
                .map(|notification| {
                    let claim_token = Uuid::new_v4();
                    self.active.lock().expect("active mutex").insert(
                        notification.key.clone(),
                        (claimant_id, claim_token, at, claim_expires_at),
                    );
                    (notification, claim_token, at, claim_expires_at)
                })
                .collect();
            Ok(claims)
        }

        async fn acknowledge_sla_notifications(
            &self,
            claimant_id: Uuid,
            claims: &[SupportSlaClaimRecord],
            at: DateTime<Utc>,
        ) -> Result<u64, SupportPersistenceError> {
            let mut acknowledged = 0;
            let mut active = self.active.lock().expect("active mutex");
            let mut delivered = self.acknowledged.lock().expect("acknowledged mutex");
            for claim in claims {
                let notification = &claim.0;
                let valid = active.get(&notification.key).is_some_and(|stored| {
                    *stored == (claimant_id, claim.1, claim.2, claim.3)
                        && claim_lease_is_live(claim, at)
                });
                if valid {
                    active.remove(&notification.key);
                    delivered.push(notification.key.clone());
                    acknowledged += 1;
                }
            }
            Ok(acknowledged)
        }
    }

    #[derive(Clone)]
    struct FakeSlaPublisher {
        calls: Arc<AtomicUsize>,
        fail_at: Option<usize>,
    }

    #[async_trait]
    impl SupportSlaNotificationPublisher for FakeSlaPublisher {
        fn is_configured(&self) -> bool {
            true
        }

        fn delivery_route(&self) -> Option<SupportSlaNotificationRoute> {
            Some(SupportSlaNotificationRoute::new("test-provider", "in_app").expect("test route"))
        }

        async fn publish(
            &self,
            _notification: &SupportSlaNotification,
        ) -> Result<(), SupportSlaNotificationPublishError> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            if self.fail_at == Some(call) {
                Err(SupportSlaNotificationPublishError::Unavailable)
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn attachment_batch_and_sla_lease_are_bounded() {
        assert_eq!(
            SupportAttachmentExpiryHandler::new(FakeExpiryStore, u32::MAX).batch_size(),
            MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE
        );
        assert!(bounded_notification_lease(Duration::zero()).is_err());
        assert!(bounded_notification_lease(Duration::days(8)).is_err());
        assert_eq!(
            bounded_notification_lease(Duration::hours(1)).expect("valid lease"),
            Duration::hours(1)
        );
    }

    #[test]
    fn provider_channel_route_is_explicit_and_bounded() {
        let route = SupportSlaNotificationRoute::new("provider-a", "email")
            .expect("valid provider/channel route");
        assert_eq!(route.provider_ref(), "provider-a");
        assert_eq!(route.channel(), "email");
        assert!(SupportSlaNotificationRoute::new("", "email").is_err());
        assert!(SupportSlaNotificationRoute::new("provider-a", "email\n").is_err());
        assert!(SupportSlaNotificationRoute::new("provider/a", "email").is_err());
    }

    #[test]
    fn access_request_binding_errors_remain_redacted() {
        let error =
            redacted_attachment_error(SupportPersistenceError::AccessRequestTicketBindingDenied);
        assert_eq!(
            error.to_string(),
            "worker configuration error: support attachment worker failed"
        );
        assert!(!error.to_string().contains("ticket"));
        assert!(!error.to_string().contains("customer"));
    }

    #[test]
    fn notification_window_uses_half_open_bounds() {
        let now = Utc::now();
        let notification = SupportSlaNotification {
            key: "sla-test".into(),
            ticket_id: Uuid::from_u128(1),
            customer_account_id: Uuid::from_u128(2),
            classification: SupportSlaNotificationClassification::DeadlineApproaching,
            audience: SupportNotificationAudience::Internal,
            deadline: now + Duration::minutes(30),
            not_before: now,
            expires_at: now + Duration::hours(1),
        };
        assert!(notification_is_within_window(&notification, now));
        assert!(!notification_is_within_window(
            &notification,
            notification.expires_at
        ));
    }

    #[tokio::test]
    async fn sla_acknowledgement_is_separate_and_failed_tail_is_retryable() {
        let now = Utc::now();
        let notifications = vec![
            SupportSlaNotification {
                key: "sla-1".into(),
                ticket_id: Uuid::from_u128(1),
                customer_account_id: Uuid::from_u128(2),
                classification: SupportSlaNotificationClassification::DeadlineApproaching,
                audience: SupportNotificationAudience::Internal,
                deadline: now + Duration::minutes(30),
                not_before: now - Duration::seconds(1),
                expires_at: now + Duration::hours(1),
            },
            SupportSlaNotification {
                key: "sla-2".into(),
                ticket_id: Uuid::from_u128(1),
                customer_account_id: Uuid::from_u128(2),
                classification: SupportSlaNotificationClassification::DeadlineApproaching,
                audience: SupportNotificationAudience::Internal,
                deadline: now + Duration::minutes(30),
                not_before: now - Duration::seconds(1),
                expires_at: now + Duration::hours(1),
            },
        ];
        let source = FakeSlaSource {
            pending: Arc::new(Mutex::new(notifications)),
            acknowledged: Arc::new(Mutex::new(Vec::new())),
            active: Arc::new(Mutex::new(HashMap::new())),
        };
        let acknowledged = Arc::clone(&source.acknowledged);
        let handler = SupportSlaNotificationHandler::new_with_publisher(
            source,
            FakeSlaPublisher {
                calls: Arc::new(AtomicUsize::new(0)),
                fail_at: Some(1),
            },
            10,
        )
        .expect("bounded SLA handler");

        let report = handler
            .run_once(Uuid::from_u128(99))
            .await
            .expect("provider failure is a durable retry report");
        assert_eq!(report.claimed, 2);
        assert_eq!(report.published, 1);
        assert_eq!(report.processed, 1);
        assert_eq!(report.retried, 1);
        assert_eq!(
            acknowledged.lock().expect("acknowledged mutex").as_slice(),
            ["sla-1"]
        );
    }

    #[tokio::test]
    async fn sla_claim_validation_rejects_duplicate_and_malformed_rows_before_publish() {
        let at = Utc::now();
        let valid = |key: &str| SupportSlaNotification {
            key: key.into(),
            ticket_id: Uuid::from_u128(1),
            customer_account_id: Uuid::from_u128(2),
            classification: SupportSlaNotificationClassification::DeadlineApproaching,
            audience: SupportNotificationAudience::Internal,
            deadline: at + Duration::minutes(30),
            not_before: at - Duration::seconds(1),
            expires_at: at + Duration::hours(1),
        };
        let source = FakeSlaSource {
            pending: Arc::new(Mutex::new(vec![valid("duplicate"), valid("duplicate")])),
            acknowledged: Arc::new(Mutex::new(Vec::new())),
            active: Arc::new(Mutex::new(HashMap::new())),
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let handler = SupportSlaNotificationHandler::new_with_publisher(
            source,
            FakeSlaPublisher {
                calls: Arc::clone(&calls),
                fail_at: None,
            },
            10,
        )
        .expect("bounded SLA handler");
        assert!(handler.run_once_at(at).await.is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 0);

        let malformed = SupportSlaNotification {
            ticket_id: Uuid::nil(),
            ..valid("malformed")
        };
        let source = FakeSlaSource {
            pending: Arc::new(Mutex::new(vec![malformed])),
            acknowledged: Arc::new(Mutex::new(Vec::new())),
            active: Arc::new(Mutex::new(HashMap::new())),
        };
        let handler = SupportSlaNotificationHandler::new_with_publisher(
            source,
            FakeSlaPublisher {
                calls: Arc::clone(&calls),
                fail_at: None,
            },
            10,
        )
        .expect("bounded SLA handler");
        assert!(handler.run_once_at(at).await.is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn sla_acknowledgement_rejects_replayed_claim_after_reclaim() {
        let at = Utc::now();
        let notification = SupportSlaNotification {
            key: "replayed-claim".into(),
            ticket_id: Uuid::from_u128(1),
            customer_account_id: Uuid::from_u128(2),
            classification: SupportSlaNotificationClassification::DeadlineApproaching,
            audience: SupportNotificationAudience::Internal,
            deadline: at + Duration::minutes(30),
            not_before: at - Duration::seconds(1),
            expires_at: at + Duration::hours(1),
        };
        let source = FakeSlaSource {
            pending: Arc::new(Mutex::new(vec![notification.clone()])),
            acknowledged: Arc::new(Mutex::new(Vec::new())),
            active: Arc::new(Mutex::new(HashMap::new())),
        };
        let worker_id = Uuid::from_u128(99);
        let first = source
            .claim_due_sla_notifications(worker_id, at, 10)
            .await
            .expect("first claim");
        source
            .pending
            .lock()
            .expect("pending mutex")
            .push(notification);
        let reclaimed_at = at + Duration::minutes(16);
        let second = source
            .claim_due_sla_notifications(worker_id, reclaimed_at, 10)
            .await
            .expect("reclaimed claim");
        assert_ne!(first[0].1, second[0].1);
        assert_eq!(
            source
                .acknowledge_sla_notifications(worker_id, &first, reclaimed_at)
                .await
                .expect("stale acknowledgement is handled"),
            0
        );
        assert_eq!(
            source
                .acknowledge_sla_notifications(worker_id, &second, reclaimed_at)
                .await
                .expect("current acknowledgement succeeds"),
            1
        );
    }

    #[tokio::test]
    async fn sla_worker_does_not_acknowledge_after_claim_lease_expiry() {
        let at = Utc::now();
        let notification = SupportSlaNotification {
            key: "expired-lease".into(),
            ticket_id: Uuid::from_u128(1),
            customer_account_id: Uuid::from_u128(2),
            classification: SupportSlaNotificationClassification::DeadlineApproaching,
            audience: SupportNotificationAudience::Internal,
            deadline: at + Duration::minutes(30),
            not_before: at - Duration::seconds(1),
            expires_at: at + Duration::hours(1),
        };
        let source = FakeSlaSource {
            pending: Arc::new(Mutex::new(vec![notification])),
            acknowledged: Arc::new(Mutex::new(Vec::new())),
            active: Arc::new(Mutex::new(HashMap::new())),
        };
        let acknowledged = Arc::clone(&source.acknowledged);
        let handler = SupportSlaNotificationHandler::new_with_publisher(
            source,
            FakeSlaPublisher {
                calls: Arc::new(AtomicUsize::new(0)),
                fail_at: None,
            },
            10,
        )
        .expect("bounded SLA handler");
        let report = handler
            .run_once_with_times(Uuid::from_u128(99), at, at + Duration::minutes(16))
            .await
            .expect("expired lease remains retryable");
        assert_eq!(report.published, 1);
        assert_eq!(report.processed, 0);
        assert_eq!(report.retried, 1);
        assert!(acknowledged.lock().expect("acknowledged mutex").is_empty());
    }

    #[derive(Clone, Copy)]
    struct OverAcknowledgingSource;

    #[async_trait]
    impl SupportSlaNotificationSource for OverAcknowledgingSource {
        fn supports_durable_claim_identity(&self) -> bool {
            true
        }

        async fn claim_due_sla_notifications(
            &self,
            claimant_id: Uuid,
            at: DateTime<Utc>,
            _batch_size: u32,
        ) -> Result<Vec<SupportSlaClaimRecord>, SupportPersistenceError> {
            Ok(vec![(
                SupportSlaNotification {
                    key: "over-acknowledged".into(),
                    ticket_id: Uuid::from_u128(1),
                    customer_account_id: Uuid::from_u128(2),
                    classification: SupportSlaNotificationClassification::DeadlineApproaching,
                    audience: SupportNotificationAudience::Internal,
                    deadline: at + Duration::minutes(30),
                    not_before: at,
                    expires_at: at + Duration::hours(1),
                },
                claimant_id,
                at,
                at + Duration::minutes(15),
            )])
        }

        async fn acknowledge_sla_notifications(
            &self,
            _claimant_id: Uuid,
            claims: &[SupportSlaClaimRecord],
            _at: DateTime<Utc>,
        ) -> Result<u64, SupportPersistenceError> {
            Ok(claims.len() as u64 + 1)
        }
    }

    #[tokio::test]
    async fn sla_worker_rejects_acknowledgements_above_live_claim_count() {
        let at = Utc::now();
        let handler = SupportSlaNotificationHandler::new_with_publisher(
            OverAcknowledgingSource,
            FakeSlaPublisher {
                calls: Arc::new(AtomicUsize::new(0)),
                fail_at: None,
            },
            10,
        )
        .expect("bounded SLA handler");

        let error = handler
            .run_once_at_for_worker(Uuid::from_u128(99), at)
            .await
            .expect_err("an acknowledgement above the live claim count must fail closed");
        assert!(error.to_string().contains("live claimed batch"));
    }
}
